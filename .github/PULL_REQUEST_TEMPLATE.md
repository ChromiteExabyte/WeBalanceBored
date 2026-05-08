## What this PR does

<!-- One or two sentences. The "why" matters more than the "what" — git
shows the what. -->

## Test plan

<!-- Tick the boxes that apply.

For pure code changes (protocol crate, filters, math): unit tests are
the bar.

For I/O / bridge / pair changes: paste hardware-test output, since CI
can't reach a real board. -->

- [ ] `cargo test --workspace` — green
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` — green
- [ ] `cargo fmt --all -- --check` — green
- [ ] Tested on real Balance Board: <!-- describe what you ran and what you saw -->
- [ ] Tested with vJoy installed: <!-- which device ID, which game, etc -->

## License

<!-- We use a deliberate split:
- balance-board-protocol, balance-board-io: MPL-2.0
- balance-board-bridge, balance-board-pair: GPL-3.0-or-later

By submitting this PR you agree your changes are licensed under the
SPDX identifier of the affected crate. -->

- [ ] I'm OK with my contribution being licensed under the affected
      crate's existing SPDX identifier.

## Related

<!-- Link any issues this fixes, or that gave you the idea. -->
