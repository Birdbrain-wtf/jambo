# Jambo

A reduced JAM client, written to answer one question: what is the smallest network that can run JAM services when the validators are the community's own members, seated by membership rather than by stake? A validator is a key the chain knows, not a person it names.

It runs unmodified JAM service blobs, built with Parity's `jam-pvm-build`, on a chain that keeps only what that case needs. The example service in `services/seeds-register` is a membership register: a work item carries a community id and a member key, refine checks its shape, and accumulate records the member once and keeps a count.

This is lab code. It holds no value and is connected to nothing live.

## Part of Seeds

Jambo is a second engine for [Seeds](https://github.com/Birdbrain-wtf/seeds), a membership design where people are admitted by the members who saw them take part, and where new units appear only when a contribution holds up. Joining mints nothing. The rules live in the Seeds chain's one pallet, [`chain/pallet`](https://github.com/Birdbrain-wtf/seeds/tree/main/chain/pallet), which runs today on a FRAME chain. The papers are [Seeds: A Store of Values](https://github.com/Birdbrain-wtf/publications/tree/main/content/articles/seeds-a-store-of-values) and [Seeds: Protocol Specification](https://github.com/Birdbrain-wtf/publications/tree/main/content/articles/seeds-protocol-specification). Jambo runs the same rules as JAM services, one job at a time, and the register here is the first: the admission step, before witnesses.

Seeds' [layer table](https://github.com/Birdbrain-wtf/seeds#the-layers) shows where each piece sits, and its [`TRUST.md`](https://github.com/Birdbrain-wtf/seeds/blob/main/TRUST.md) says what each one is trusted with, this client included. Nothing in this repo restates either. When the two disagree, the pallet is right and this is behind.

## Run it

```bash
bash scripts/demo.sh                    # builds the client and the example service, then runs the demo
bash scripts/demo.sh --validators 21    # any multiple of 3
```

The demo admits two members, resubmits one (the count stays at 2), sends a malformed entry, then takes validators offline. With up to a third offline, blocks keep finalising. One more and blocks are still produced but nothing finalises. When everyone returns, the missing validators replay what they missed and finality resumes. At every step it prints how many distinct state roots the online validators hold, which should always be 1.

Building the service needs `rustup` with the `nightly-2025-05-10` toolchain. The script installs it and `jam-pvm-build` if they are missing.

## What it keeps from JAM

- **Services**, with refine and accumulate as separate entry points, using the same program blob format, entry points, argument encoding and host-call numbering as Parity's `polkajam`. A blob built for one runs on the other.
- **Guarantees.** Each core has three guarantors, rotating every 4 slots. Two of them must refine a package on their own state, reach the same result and sign it before it can go into a block.
- **Re-execution.** Every validator re-runs accumulate on its own copy of the state and votes only if it reaches the root in the header.

## What it leaves out, on purpose

| JAM | Here |
| --- | --- |
| Safrole block production with anonymous tickets | Round-robin authors from a named validator set |
| GRANDPA finality | One round of signatures over the post-state. More than two thirds finalises the block and everything before it |
| Fork choice | None. A known set with one author per slot does not fork unless a validator signs two blocks for one slot, which is not handled yet |
| Erasure coding, data availability, audits | None. Guarantors are trusted to have kept the package |
| Merkle state trie | One hash over the sorted state, enough for validators to agree and too little for light clients |
| Networking | All validators run in one process for now |
| Host calls | `gas`, `fetch`, `read`, `write`, `log`. Everything else returns WHAT |

Each row is a choice that suits a small trusted network and has to be revisited before strangers can run a node.

## The VM boundary

Everything above `src/vm.rs` is independent of the VM. A backend gets a code blob, an entry point (refine or accumulate), an argument buffer and a gas budget. It calls back into the host through `Host::ecall` with a host-call id, at most six `u64` arguments and one `u64` return. That is the whole contract.

There are two backends:

- `pvm` (`src/pvm.rs`) runs JAM blobs on PolkaVM, Parity's RISC-V variant with its own encoding.
- `riscv` (`src/riscv.rs`) is empty. It is where a standard RV64 interpreter goes, so services can be built with ordinary RISC-V toolchains. The file header describes the calling convention it should follow. Run it with `--vm riscv`.

PolkaVM stays the default, so the client keeps running the same blobs as other JAM clients. Standard RISC-V comes in beside it as an option, not a replacement.

Why standard RISC-V: [Alley](https://alleyos.org), a base layer with no coin of its own, runs its programs on a virtual machine built on standard 64-bit RISC-V (RV64E). A standard backend here is the development direction for running the same membership rules on Alley as well as on JAM. It is not built yet, and nothing here runs on Alley today.

## Next

Each step brings one more of the pallet's jobs across.

1. A members-only authoriser, so only a member's signature can get work onto a core. The pallet's `OnlyMembers` gate.
2. Two-witness admission: refine checks two existing members' signatures, accumulate applies the allowances and caps. The pallet's `witness`.
3. Validators as separate processes on separate machines.
4. The standard RISC-V backend, the route towards running on Alley.
5. Running the JAM conformance vectors against the parts we kept.

## Licence

Apache-2.0. It depends on Parity's `polkavm`, `jam-types` and `jam-program-blob-common` crates, also Apache-2.0.
