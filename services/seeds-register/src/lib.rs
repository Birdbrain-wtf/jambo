//! Seeds register: the first Seeds-chain rule as a JAM service.
//! A work item carries a 4-byte community id and a 32-byte member key.
//! Refine checks the shape; accumulate records the member once and counts them.
#![no_std]
extern crate alloc;

use alloc::vec::Vec;
use jam_pvm_common::{accumulate::*, declare_service, info, Service};
use jam_types::{AccumulateItem, CoreIndex, Hash, ServiceId, Slot, WorkOutput, WorkPackageHash, WorkPayload};

const ENTRY: usize = 36;

struct SeedsRegister;
declare_service!(SeedsRegister);

impl Service for SeedsRegister {
	fn refine(_: CoreIndex, _: usize, _: ServiceId, payload: WorkPayload, _: WorkPackageHash) -> WorkOutput {
		let p = payload.0;
		info!(target = "seeds", "refine {} bytes", p.len());
		if p.len() == ENTRY { WorkOutput(p) } else { WorkOutput(Vec::new()) }
	}

	fn accumulate(slot: Slot, _: ServiceId, _: usize) -> Option<Hash> {
		info!(target = "seeds", "accumulate {} items", accumulate_items().len());
		for item in accumulate_items() {
			let AccumulateItem::WorkItem(rec) = item else { continue };
			let Ok(out) = rec.result else {
				info!(target = "seeds", "refine failed");
				continue
			};
			info!(target = "seeds", "item output {} bytes", out.0.len());
			if out.0.len() != ENTRY { continue }
			let mut key = Vec::with_capacity(1 + ENTRY);
			key.push(b'm');
			key.extend_from_slice(&out.0);
			if get_storage(&key).is_some() { continue }
			if let Err(e) = set_storage(&key, &slot.to_le_bytes()) {
				info!(target = "seeds", "set_storage failed {:?}", e);
				continue
			}
			let n = get_storage(b"count")
				.and_then(|v| v.try_into().ok())
				.map(u64::from_le_bytes)
				.unwrap_or(0) + 1;
			let _ = set_storage(b"count", &n.to_le_bytes());
			info!(target = "seeds", "admitted member {} at slot {}", n, slot);
		}
		None
	}
}
