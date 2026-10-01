//! The VM boundary. Everything above this file is VM-agnostic: a backend gets a code
//! blob, an entry point, an argument buffer and a gas budget, and calls back into the
//! host through `Host::ecall` with at most six u64 arguments and one u64 return. That
//! is the whole contract a second backend (standard RISC-V) has to meet.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
	Refine,
	Accumulate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exit {
	/// Normal return: the bytes the program pointed at on exit.
	Halt(Vec<u8>),
	Panic(String),
	OutOfGas,
	/// The backend cannot run this blob at all (wrong format, not built yet).
	Unsupported(String),
}

pub trait Memory {
	fn read(&self, addr: u32, len: u32) -> Option<Vec<u8>>;
	fn write(&mut self, addr: u32, data: &[u8]) -> bool;
}

pub trait Host {
	/// One host call. `id` is the JAM host-call index (Gray Paper numbering plus 100 = log).
	/// Returning `Err` stops the program with that exit.
	fn ecall(&mut self, id: u32, args: [u64; 6], mem: &mut dyn Memory, gas: &mut i64) -> Result<u64, Exit>;
}

pub trait Vm {
	fn name(&self) -> &'static str;
	/// Returns the exit and the gas left.
	fn invoke(&self, code: &[u8], entry: Entry, args: &[u8], gas: i64, host: &mut dyn Host) -> (Exit, i64);
}

pub fn by_name(name: &str) -> Option<Box<dyn Vm>> {
	match name {
		"pvm" => Some(Box::new(crate::pvm::Pvm::new())),
		"riscv" => Some(Box::new(crate::riscv::Riscv)),
		_ => None,
	}
}
