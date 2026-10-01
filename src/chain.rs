//! The reduced chain: a trusted validator set, round-robin authors, one core, and
//! finality as soon as more than two thirds of validators have re-executed a block and
//! signed its post-state. No tickets, no fork choice, no erasure coding, no networking:
//! every validator lives in this process, holds its own copy of state, and must reach
//! the same root on its own before it votes.

use crate::host::{AccumulateHost, RefineHost};
use crate::state::{hash, Hash, State};
use crate::vm::{Entry, Exit, Vm};
use codec::Encode;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use jam_types::{
	AccumulateItem, AccumulateParams, AuthorizerHash, PayloadHash, ProtocolParameters, RefineParams, SegmentTreeRoot,
	WorkError, WorkItemRecord, WorkOutput, WorkPackageHash, WorkPayload,
};

pub const GUARANTORS_PER_CORE: usize = 3;
pub const ROTATION: u32 = 4;
const REFINE_GAS: i64 = 100_000_000;
const ACCUMULATE_GAS: i64 = 10_000_000;

#[derive(Clone, Debug, Encode)]
pub struct WorkItem {
	pub service: u32,
	pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Encode)]
pub struct ItemResult {
	pub service: u32,
	pub payload_hash: Hash,
	pub output: Result<Vec<u8>, u8>,
}

#[derive(Clone, Debug, Encode)]
pub struct WorkReport {
	pub package_hash: Hash,
	pub core: u16,
	pub slot: u32,
	pub results: Vec<ItemResult>,
}

#[derive(Clone, Debug)]
pub struct Guarantee {
	pub report: WorkReport,
	pub sigs: Vec<(u16, Signature)>,
}

#[derive(Clone, Debug, Encode)]
pub struct Header {
	pub parent: Hash,
	pub slot: u32,
	pub author: u16,
	pub reports: Hash,
	pub state_root: Hash,
}

#[derive(Clone, Debug)]
pub struct Block {
	pub header: Header,
	pub guarantees: Vec<Guarantee>,
	pub seal: Signature,
}

impl Header {
	pub fn hash(&self) -> Hash {
		hash(&self.encode())
	}
}

pub struct Validator {
	pub index: u16,
	key: SigningKey,
	pub state: State,
	pub head: Hash,
	pub height: usize,
	pub online: bool,
}

pub struct Network {
	pub vals: Vec<Validator>,
	pub keys: Vec<VerifyingKey>,
	pub blocks: Vec<Block>,
	pub finalized: usize,
	pub slot: u32,
	pending: Vec<Guarantee>,
	params: Vec<u8>,
	vm: Box<dyn Vm>,
}

fn key_for(i: usize) -> SigningKey {
	SigningKey::from_bytes(&hash(format!("//minijam//validator//{i}").as_bytes()))
}

fn report_msg(r: &WorkReport) -> Vec<u8> {
	[b"report".as_slice(), &hash(&r.encode())].concat()
}

impl Network {
	pub fn new(validators: usize, genesis: State, vm: Box<dyn Vm>) -> Self {
		assert!(validators >= 3 && validators % 3 == 0, "validator count must be a multiple of 3");
		let mut p = ProtocolParameters::tiny();
		p.val_count = validators as u16;
		// The spec ties cores to validators (three per core). We schedule work on core 0 only.
		p.core_count = (validators / GUARANTORS_PER_CORE) as u16;
		p.validate().expect("protocol parameters");
		let vals: Vec<Validator> = (0..validators)
			.map(|i| Validator {
				index: i as u16,
				key: key_for(i),
				state: genesis.clone(),
				head: [0; 32],
				height: 0,
				online: true,
			})
			.collect();
		let keys = vals.iter().map(|v| v.key.verifying_key()).collect();
		Network { vals, keys, blocks: Vec::new(), finalized: 0, slot: 0, pending: Vec::new(), params: p.encode(), vm }
	}

	pub fn threshold(&self) -> usize {
		self.vals.len() * 2 / 3 + 1
	}

	/// Which validators guarantee core 0 at `slot`: groups of three, rotating.
	pub fn guarantors(&self, slot: u32) -> Vec<usize> {
		let groups = self.vals.len() / GUARANTORS_PER_CORE;
		let g = ((slot / ROTATION) as usize) % groups;
		(g * GUARANTORS_PER_CORE..(g + 1) * GUARANTORS_PER_CORE).collect()
	}

	fn refine(&self, v: &Validator, package_hash: Hash, items: &[WorkItem], echo: bool) -> Vec<ItemResult> {
		let payloads: Vec<Vec<u8>> = items.iter().map(|i| i.payload.clone()).collect();
		items
			.iter()
			.enumerate()
			.map(|(n, item)| {
				let output = match v.state.services.get(&item.service) {
					None => Err(WorkError::BadCode as u8),
					Some(acct) => {
						let args = RefineParams {
							core_index: 0,
							item_index: n as u32,
							service_id: item.service,
							payload: WorkPayload(item.payload.clone()),
							package_hash: WorkPackageHash(package_hash),
						}
						.encode();
						let mut host = RefineHost {
							params: &self.params,
							payloads: &payloads,
							echo: echo.then(|| format!("refine  v{} #{}", v.index, item.service)),
						};
						match self.vm.invoke(&acct.code, Entry::Refine, &args, REFINE_GAS, &mut host).0 {
							Exit::Halt(out) => Ok(out),
							Exit::OutOfGas => Err(WorkError::OutOfGas as u8),
							e => {
								if echo {
									println!("    refine v{} #{}: {e:?}", v.index, item.service);
								}
								Err(WorkError::Panic as u8)
							},
						}
					},
				};
				ItemResult { service: item.service, payload_hash: hash(&item.payload), output }
			})
			.collect()
	}

	/// The assigned guarantors refine the package on their own state. The report stands
	/// once two of them produce identical results and sign it.
	pub fn submit(&mut self, items: Vec<WorkItem>) -> Result<Hash, String> {
		let slot = self.slot + 1;
		let package_hash = hash(&(&items, self.vals[0].head).encode());
		let mut agreed: Option<WorkReport> = None;
		let mut sigs = Vec::new();
		for (n, i) in self.guarantors(slot).into_iter().enumerate() {
			let v = &self.vals[i];
			if !v.online {
				continue
			}
			let results = self.refine(v, package_hash, &items, n == 0);
			let report = WorkReport { package_hash, core: 0, slot, results };
			match &agreed {
				None => agreed = Some(report.clone()),
				Some(a) if a.encode() != report.encode() => {
					println!("    guarantor v{i} disagrees, not signing");
					continue
				},
				_ => {},
			}
			sigs.push((i as u16, v.key.sign(&report_msg(&report))));
		}
		match agreed {
			Some(report) if sigs.len() >= 2 => {
				self.pending.push(Guarantee { report, sigs });
				Ok(package_hash)
			},
			_ => Err(format!("only {} guarantor(s) of core 0 online, need 2", sigs.len())),
		}
	}

	fn check_guarantee(&self, g: &Guarantee) -> bool {
		let assigned = self.guarantors(g.report.slot);
		let msg = report_msg(&g.report);
		let ok = g.sigs.iter().filter(|(i, s)| assigned.contains(&(*i as usize)) && self.keys[*i as usize].verify(&msg, s).is_ok());
		ok.count() >= 2
	}

	/// Apply a block's reports to `state`: accumulate each service once over its items.
	fn apply(&self, state: &mut State, slot: u32, guarantees: &[Guarantee], echo: Option<u16>) {
		let mut by_service: std::collections::BTreeMap<u32, Vec<Vec<u8>>> = Default::default();
		for g in guarantees {
			for r in &g.report.results {
				let result = match &r.output {
					Ok(o) => Ok(WorkOutput(o.clone())),
					Err(e) => Err(if *e == WorkError::OutOfGas as u8 { WorkError::OutOfGas } else { WorkError::Panic }),
				};
				let rec = AccumulateItem::WorkItem(WorkItemRecord {
					package: WorkPackageHash(g.report.package_hash),
					exports_root: SegmentTreeRoot([0; 32]),
					authorizer_hash: AuthorizerHash([0; 32]),
					payload: PayloadHash(r.payload_hash),
					gas_limit: ACCUMULATE_GAS as u64,
					result,
					auth_output: Default::default(),
				});
				by_service.entry(r.service).or_default().push(rec.encode());
			}
		}
		for (service, items) in by_service {
			let Some(acct) = state.services.get(&service) else { continue };
			let mut scratch = acct.clone();
			let code = acct.code.clone();
			let all: Vec<u8> = {
				let mut b = codec::Compact(items.len() as u32).encode();
				items.iter().for_each(|i| b.extend_from_slice(i));
				b
			};
			let args = AccumulateParams { slot, service_id: service, item_count: items.len() as u32 }.encode();
			let mut host = AccumulateHost {
				params: &self.params,
				items: &all,
				item_list: &items,
				account: &mut scratch,
				echo: echo.map(|v| format!("accum   v{v} #{service}")),
			};
			let (exit, _) = self.vm.invoke(&code, Entry::Accumulate, &args, ACCUMULATE_GAS, &mut host);
			match exit {
				Exit::Halt(_) => {
					state.services.insert(service, scratch);
				},
				e if echo.is_some() => println!("    accumulate #{service} reverted: {e:?}"),
				_ => {},
			}
		}
		state.slot = slot;
	}

	/// One slot. The scheduled author (if online) builds, executes and seals a block;
	/// every other online validator re-executes it and votes if it reaches the same root.
	pub fn step(&mut self) -> Option<(Hash, usize, bool)> {
		self.slot += 1;
		let slot = self.slot;
		let author = (slot as usize) % self.vals.len();
		if !self.vals[author].online {
			println!("  slot {slot}: author v{author} offline, no block");
			return None
		}
		self.catch_up();
		let guarantees: Vec<Guarantee> = std::mem::take(&mut self.pending).into_iter().filter(|g| self.check_guarantee(g)).collect();
		let mut post = self.vals[author].state.clone();
		self.apply(&mut post, slot, &guarantees, Some(author as u16));
		let header = Header {
			parent: self.vals[author].head,
			slot,
			author: author as u16,
			reports: hash(&guarantees.iter().map(|g| g.report.clone()).collect::<Vec<_>>().encode()),
			state_root: post.root(),
		};
		let h = header.hash();
		let seal = self.vals[author].key.sign(&h);
		let block = Block { header, guarantees, seal };
		self.blocks.push(block.clone());
		let mut votes = 0;
		for i in 0..self.vals.len() {
			if !self.vals[i].online {
				continue
			}
			if self.import(i, &block) {
				votes += 1;
			}
		}
		let final_now = votes >= self.threshold();
		if final_now {
			self.finalized = self.blocks.len();
		}
		Some((h, votes, final_now))
	}

	/// Validator `i` checks the seal and author, re-executes, and votes on a matching root.
	fn import(&mut self, i: usize, block: &Block) -> bool {
		let hd = &block.header;
		let expected = (hd.slot as usize) % self.vals.len();
		if hd.author as usize != expected || self.keys[expected].verify(&hd.hash(), &block.seal).is_err() {
			return false
		}
		if hd.parent != self.vals[i].head || !block.guarantees.iter().all(|g| self.check_guarantee(g)) {
			return false
		}
		let mut post = self.vals[i].state.clone();
		self.apply(&mut post, hd.slot, &block.guarantees, None);
		if post.root() != hd.state_root {
			println!("    v{i}: state root mismatch, refusing to vote");
			return false
		}
		let v = &mut self.vals[i];
		v.state = post;
		v.head = hd.hash();
		v.height += 1;
		true
	}

	/// Validators that were offline replay the blocks they missed before taking part.
	fn catch_up(&mut self) {
		for i in 0..self.vals.len() {
			if !self.vals[i].online {
				continue
			}
			while self.vals[i].height < self.blocks.len() {
				let b = self.blocks[self.vals[i].height].clone();
				if !self.import(i, &b) {
					break
				}
			}
		}
	}

	pub fn storage(&self, v: usize, service: u32, key: &[u8]) -> Option<Vec<u8>> {
		self.vals[v].state.services.get(&service)?.storage.get(key).cloned()
	}
}
