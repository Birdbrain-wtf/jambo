//! minijam: a reduced JAM client for Birdbrain Lab. See ../README.md.

mod chain;
mod host;
mod pvm;
mod riscv;
mod state;
mod vm;

use chain::{Network, WorkItem};
use state::{Account, State};
use std::time::Instant;

const SERVICE: u32 = 1;

fn arg(args: &[String], name: &str) -> Option<String> {
	args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn member(community: u32, name: &str) -> Vec<u8> {
	let mut p = community.to_le_bytes().to_vec();
	p.extend_from_slice(&state::hash(name.as_bytes()));
	p
}

fn count(net: &Network, v: usize) -> u64 {
	net.storage(v, SERVICE, b"count").and_then(|b| b.try_into().ok()).map(u64::from_le_bytes).unwrap_or(0)
}

fn step(net: &mut Network) {
	if let Some((h, votes, fin)) = net.step() {
		println!(
			"  slot {}: block {} by v{}, {votes}/{} votes, {}",
			net.slot,
			&hex::encode(h)[..12],
			(net.slot as usize) % net.vals.len(),
			net.vals.len(),
			if fin { "final" } else { "NOT final" }
		);
	}
}

fn agree(net: &Network) {
	let online: Vec<&chain::Validator> = net.vals.iter().filter(|v| v.online).collect();
	let roots: std::collections::BTreeSet<_> = online.iter().map(|v| v.state.root()).collect();
	println!(
		"  {} online validators, {} distinct state root(s), count = {}",
		online.len(),
		roots.len(),
		count(net, online[0].index as usize)
	);
}

fn main() {
	let args: Vec<String> = std::env::args().collect();
	let path = arg(&args, "--service")
		.or_else(|| std::env::var("MINIJAM_SERVICE").ok())
		.unwrap_or("target/seeds-register-service.jam".into());
	let n: usize = arg(&args, "--validators").and_then(|s| s.parse().ok()).unwrap_or(6);
	let vm_name = arg(&args, "--vm").unwrap_or("pvm".into());
	let Some(vm) = vm::by_name(&vm_name) else {
		eprintln!("unknown --vm {vm_name} (pvm | riscv)");
		std::process::exit(2)
	};
	let code = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));

	let mut genesis = State::default();
	genesis.services.insert(SERVICE, Account { code, balance: 1 << 40, ..Default::default() });
	let mut net = Network::new(n, genesis, vm);
	println!(
		"minijam: {n} validators, 1 core, vm={vm_name}, finality at {}/{n}, service #{SERVICE} from {path}",
		net.threshold()
	);

	println!("\n1. Admit alice and bob");
	let t = Instant::now();
	for who in ["alice", "bob"] {
		match net.submit(vec![WorkItem { service: SERVICE, payload: member(1, who) }]) {
			Ok(h) => println!("  {who}: guaranteed, package {}", &hex::encode(h)[..12]),
			Err(e) => println!("  {who}: {e}"),
		}
	}
	step(&mut net);
	println!("  submit to final: {:?} of compute, 1 slot", t.elapsed());
	agree(&net);

	println!("\n2. Resubmit alice (must not count twice)");
	let _ = net.submit(vec![WorkItem { service: SERVICE, payload: member(1, "alice") }]);
	step(&mut net);
	agree(&net);

	println!("\n3. A malformed entry (refine rejects it, accumulate ignores it)");
	let _ = net.submit(vec![WorkItem { service: SERVICE, payload: b"not a member".to_vec() }]);
	step(&mut net);
	agree(&net);

	let tolerated = (n - 1) / 3;
	println!("\n4. Liveness: {tolerated} validator(s) offline");
	for i in 0..tolerated {
		net.vals[n - 1 - i].online = false;
	}
	for _ in 0..n {
		step(&mut net);
	}

	println!("\n5. One more offline: blocks continue, finality stops");
	net.vals[n - 1 - tolerated].online = false;
	match net.submit(vec![WorkItem { service: SERVICE, payload: member(1, "carol") }]) {
		Ok(_) => println!("  carol: guaranteed"),
		Err(e) => println!("  carol: not guaranteed, {e}"),
	}
	for _ in 0..3 {
		step(&mut net);
	}
	println!("  finalized height {} of {}", net.finalized, net.blocks.len());

	println!("\n6. Everyone back: the missing validators replay and finality resumes");
	net.vals.iter_mut().for_each(|v| v.online = true);
	step(&mut net);
	println!("  finalized height {} of {}", net.finalized, net.blocks.len());
	agree(&net);
}

