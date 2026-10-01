//! Host calls, numbered as in `jam-pvm-common` (Gray Paper order, plus 100 = log).
//! Only what a refine/accumulate service needs today is implemented; everything else
//! answers WHAT, which a well-behaved service treats as "not available here".

use crate::state::Account;
use crate::vm::{Exit, Host, Memory};

pub const NONE: u64 = u64::MAX;
pub const WHAT: u64 = u64::MAX - 1;

const GAS: u32 = 0;
const FETCH: u32 = 1;
const READ: u32 = 3;
const WRITE: u32 = 4;
const LOG: u32 = 100;

const F_PARAMS: u64 = 0;
const F_ANY_PAYLOAD: u64 = 13;
const F_ACC_ITEMS: u64 = 14;
const F_ANY_ACC_ITEM: u64 = 15;

fn mem_read(mem: &dyn Memory, ptr: u64, len: u64) -> Result<Vec<u8>, Exit> {
	mem.read(ptr as u32, len as u32).ok_or_else(|| Exit::Panic(format!("bad read {ptr:#x}+{len}")))
}

/// Gray Paper fetch semantics: copy v[off..off+len] to ptr, return |v|.
fn give(mem: &mut dyn Memory, v: Option<&[u8]>, ptr: u64, off: u64, len: u64) -> Result<u64, Exit> {
	let Some(v) = v else { return Ok(NONE) };
	let s = (off as usize).min(v.len());
	let e = (s + len as usize).min(v.len());
	if e > s && !mem.write(ptr as u32, &v[s..e]) {
		return Err(Exit::Panic(format!("bad write {ptr:#x}")))
	}
	Ok(v.len() as u64)
}

fn log(prefix: &str, mem: &dyn Memory, a: [u64; 6]) -> Result<u64, Exit> {
	let target = mem_read(mem, a[1], a[2])?;
	let text = mem_read(mem, a[3], a[4])?;
	println!("    {prefix} {}: {}", String::from_utf8_lossy(&target), String::from_utf8_lossy(&text));
	Ok(0)
}

pub struct RefineHost<'a> {
	pub params: &'a [u8],
	pub payloads: &'a [Vec<u8>],
	pub echo: Option<String>,
}

impl Host for RefineHost<'_> {
	fn ecall(&mut self, id: u32, a: [u64; 6], mem: &mut dyn Memory, gas: &mut i64) -> Result<u64, Exit> {
		match id {
			GAS => Ok(*gas as u64),
			FETCH => {
				let v = match a[3] {
					F_PARAMS => Some(self.params),
					F_ANY_PAYLOAD => self.payloads.get(a[4] as usize).map(|p| &p[..]),
					_ => None,
				};
				give(mem, v, a[0], a[1], a[2])
			},
			LOG => match &self.echo {
				Some(p) => log(p, mem, a),
				None => Ok(0),
			},
			_ => Ok(WHAT),
		}
	}
}

pub struct AccumulateHost<'a> {
	pub params: &'a [u8],
	pub items: &'a [u8],
	pub item_list: &'a [Vec<u8>],
	pub account: &'a mut Account,
	pub echo: Option<String>,
}

impl Host for AccumulateHost<'_> {
	fn ecall(&mut self, id: u32, a: [u64; 6], mem: &mut dyn Memory, gas: &mut i64) -> Result<u64, Exit> {
		match id {
			GAS => Ok(*gas as u64),
			FETCH => {
				let v = match a[3] {
					F_PARAMS => Some(self.params),
					F_ACC_ITEMS => Some(self.items),
					F_ANY_ACC_ITEM => self.item_list.get(a[4] as usize).map(|p| &p[..]),
					_ => None,
				};
				give(mem, v, a[0], a[1], a[2])
			},
			READ => {
				// Only our own storage for now (service == u64::MAX or self).
				let key = mem_read(mem, a[1], a[2])?;
				let v = self.account.storage.get(&key).cloned();
				give(mem, v.as_deref(), a[3], a[4], a[5])
			},
			WRITE => {
				let key = mem_read(mem, a[0], a[1])?;
				let old = if a[3] == 0 {
					self.account.storage.remove(&key)
				} else {
					let val = mem_read(mem, a[2], a[3])?;
					self.account.storage.insert(key, val)
				};
				Ok(old.map(|v| v.len() as u64).unwrap_or(NONE))
			},
			LOG => match &self.echo {
				Some(p) => log(p, mem, a),
				None => Ok(0),
			},
			_ => Ok(WHAT),
		}
	}
}
