//! Standard RISC-V backend: the slot Wei offered to fill.
//!
//! The contract is `crate::vm::Vm`. A backend here would load an ordinary RV64 ELF built
//! by a stock toolchain, enter it at the `refine` / `accumulate` symbols with
//! `a0 = args_ptr, a1 = args_len`, map `ecall` with `a7 = host-call id` and `a0..a5` as
//! arguments onto `Host::ecall`, and on return read `(a0, a1)` as the output pointer and
//! length. Gas is the backend's own choice of metering, reported back as the remainder.
//! Nothing above the VM boundary changes when it lands.

use crate::vm::{Entry, Exit, Host, Vm};

pub struct Riscv;

impl Vm for Riscv {
	fn name(&self) -> &'static str {
		"riscv"
	}
	fn invoke(&self, _: &[u8], _: Entry, _: &[u8], gas: i64, _: &mut dyn Host) -> (Exit, i64) {
		(Exit::Unsupported("the standard RISC-V backend is not written yet".into()), gas)
	}
}
