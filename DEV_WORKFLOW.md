# Developer Workflow & Troubleshooting Guide

This guide is designed to help you build, test, and troubleshoot the Omni language toolchain, particularly when dealing with compiled binaries during active development.

## The Local Development Loop

When you edit code in `omni-compiler` or `omni-vm`, following these steps will ensure your changes reflect in the `omni` executable you run in the terminal.

### 1. Build and Test Locally First
To test your changes without affecting your global system, use `cargo run`. This runs the newly compiled code directly from the project directory.

```powershell
# Inside the omni-lang directory
cargo run --bin omni-cli -- run .\examples\student.omni
```

### 2. Updating the Global `omni` Command
If you want the terminal command `omni run <file>` to use your latest changes everywhere, you **must** instruct Cargo to reinstall the binary globally based on your local path.

Because `omni-lang` is a Cargo Workspace, you must pass the specific nested CLI package to the install command:

```powershell
# This replaces the old omni.exe in ~/.cargo/bin with your freshly compiled version
cargo install --path omni-cli
```

> [!WARNING]
> **Common Trap**: If you only run `cargo build`, the code is compiled, but your terminal's `omni` command will still point to an older, cached executable in your global path, making it look as if your fixes "didn't apply".

## Quick Troubleshooting Checklist

If your Omni script behaves differently than your updated Rust code:
1. **Did you save all your Rust files?**
2. **Did you run `cargo install --path omni-cli`?**
3. **If running `cargo run`, did you pass `--bin omni-cli --` before the `omni` arguments?**
4. **Is there a syntactical issue in the `.omni` script you are compiling?** Run `cargo run --bin omni-cli -- check <file.omni>` to perform a standalone type-check and find out early.
