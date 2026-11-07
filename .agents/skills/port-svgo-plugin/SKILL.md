---
name: port-svgo-plugin
description: Port one SVGO plugin to Rust by translating its JavaScript source and fixtures into a registered Rust module verified by tests and corpus parity checks.
---

# Port SVGO Plugin

Use this skill to port a single SVGO plugin from JavaScript to Rust in this repository.

1. Read the JavaScript plugin and its fixtures in `reference/svgo` to capture the plugin behavior, options, and expected input/output cases.
2. Write the corresponding Rust module following the existing plugin patterns in this repository.
3. Register the new Rust module so it is discoverable through the normal plugin registration path.
4. Port the JavaScript fixtures as Rust tests with equivalent inputs, options, and expected outputs.
5. Run `cargo test` and fix failures until the ported tests pass.
6. Run the parity check over the corpus and resolve any mismatches between the JavaScript and Rust behavior before reporting completion.
