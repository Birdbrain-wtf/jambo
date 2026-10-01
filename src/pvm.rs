//! PolkaVM backend: runs JAM program blobs exactly as `jam-pvm-build` emits them.

use crate::vm::{Entry, Exit, Host, Memory, Vm};
use jam_program_blob_common::ProgramBlob as JamBlob;
use polkavm::{
	BackendKind, Config, Engine, GasMeteringKind, InterruptKind, ModuleConfig, ProgramBlob, ProgramCounter, ProgramParts,
	RawInstance, Reg,
};

/// Gray Paper charge per host call.
const HOST_CALL_GAS: i64 = 10;

pub struct Pvm {
	engine: Engine,
}

impl Pvm {
	pub fn new() -> Self {
		let mut config = Config::from_env().expect("polkavm config");
		if std::env::var("POLKAVM_BACKEND").is_err() {
			config.set_backend(Some(BackendKind::Interpreter));
		}
		Pvm { engine: Engine::new(&config).expect("polkavm engine") }
	}
}

struct Mem<'a>(&'a mut RawInstance);
impl Memory for Mem<'_> {
	fn read(&self, addr: u32, len: u32) -> Option<Vec<u8>> {
		self.0.read_memory(addr, len).ok()
	}
	fn write(&mut self, addr: u32, data: &[u8]) -> bool {
		self.0.write_memory(addr, data).is_ok()
	}
}

impl Vm for Pvm {
	fn name(&self) -> &'static str {
		"pvm"
	}

	fn invoke(&self, code: &[u8], entry: Entry, args: &[u8], gas: i64, host: &mut dyn Host) -> (Exit, i64) {
		let Some(blob) = JamBlob::from_bytes(code) else {
			return (Exit::Unsupported("not a JAM program blob".into()), gas)
		};
		let parts: ProgramParts = blob.into();
		let program = match ProgramBlob::from_parts(parts) {
			Ok(p) => p,
			Err(e) => return (Exit::Unsupported(format!("bad PVM code: {e}")), gas),
		};
		let mut mc = ModuleConfig::new();
		mc.set_gas_metering(Some(GasMeteringKind::Sync));
		mc.set_aux_data_size(args.len().max(1) as u32);
		let module = match polkavm::Module::from_blob(&self.engine, &mc, program) {
			Ok(m) => m,
			Err(e) => return (Exit::Unsupported(format!("module: {e}")), gas),
		};
		let mut inst = module.instantiate().expect("instantiate");
		let aux = module.memory_map().aux_data_address();
		inst.write_memory(aux, args).expect("write args");
		// Gray Paper: refine enters at 0, accumulate at 5.
		let pc = match entry {
			Entry::Refine => 0,
			Entry::Accumulate => 5,
		};
		inst.prepare_call_untyped(ProgramCounter(pc), &[aux as u64, args.len() as u64]);
		inst.set_gas(gas);
		loop {
			let kind = match inst.run() {
				Ok(k) => k,
				Err(e) => return (Exit::Panic(format!("vm error: {e}")), inst.gas()),
			};
			match kind {
				InterruptKind::Finished => {
					let (ptr, len) = (inst.reg(Reg::A0) as u32, inst.reg(Reg::A1) as u32);
					let out = if len == 0 { Vec::new() } else { inst.read_memory(ptr, len).unwrap_or_default() };
					return (Exit::Halt(out), inst.gas())
				},
				InterruptKind::Ecalli(id) => {
					let mut g = inst.gas() - HOST_CALL_GAS;
					if g < 0 {
						return (Exit::OutOfGas, 0)
					}
					let a = [Reg::A0, Reg::A1, Reg::A2, Reg::A3, Reg::A4, Reg::A5].map(|r| inst.reg(r));
					let r = host.ecall(id, a, &mut Mem(&mut inst), &mut g);
					inst.set_gas(g);
					match r {
						Ok(v) => inst.set_reg(Reg::A0, v),
						Err(exit) => return (exit, g),
					}
				},
				InterruptKind::NotEnoughGas => return (Exit::OutOfGas, 0),
				other => return (Exit::Panic(format!("{other:?}")), inst.gas()),
			}
		}
	}
}
