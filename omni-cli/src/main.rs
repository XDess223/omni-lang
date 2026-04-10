// omni-cli/src/main.rs
// The `omni` command-line tool.
//
// Usage:
//   omni run   <file.omni>          — Compile and execute an Omni source file
//   omni check <file.omni>          — Typecheck only, print errors (no execution)
//   omni help                       — Print this message

use std::{env, fs, process};

use omni_compiler::{compile, codegen::CodeGen};
use omni_vm::vm::Vm;

fn main() {
    let args: Vec<String> = env::args().collect();

    // ── Dispatch on sub-command ────────────────────────────────────────────
    match args.get(1).map(String::as_str) {
        Some("run")   => cmd_run(&args),
        Some("check") => cmd_check(&args),
        Some("help") | Some("--help") | Some("-h") => print_help(),
        _ => {
            eprintln!("Omni — Unknown or missing sub-command.");
            eprintln!("Run `omni help` for usage.");
            process::exit(1);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// omni run <file.omni>
// ─────────────────────────────────────────────────────────────────────────────
fn cmd_run(args: &[String]) {
    let path = require_file_arg(args, "run");

    let source = read_source(&path);

    // ── Phase 1–3: Compile (lex + parse + semantic) ───────────────────────
    let program = match compile(&source) {
        Ok(p)  => p,
        Err(e) => {
            eprintln!("❌  Compile error in '{}':\n{}", path, e);
            process::exit(1);
        }
    };

    // ── Phase 4: Bytecode generation ─────────────────────────────────────
    let mut gen = CodeGen::new();
    gen.generate(&program);
    let compiled = gen.output;

    // ── Phase 5: Execute in the VM ────────────────────────────────────────
    // Convention: a method named "Main::main" is the entry point.
    // If this method does not exist, try any available method for demo purposes.
    let entry = find_entry(&compiled);
    match entry {
        Some(key) => {
            println!("▶  Running '{}' …\n", key);
            let mut vm = Vm::new(compiled);
            match vm.execute(&key) {
                Ok(Some(val)) => println!("\n✅  Exited with: {:?}", val),
                Ok(None)      => println!("\n✅  Program completed."),
                Err(e)        => {
                    eprintln!("\n❌  Runtime error: {:?}", e);
                    process::exit(1);
                }
            }
        }
        None => {
            let mut keys: Vec<_> = compiled.methods.keys().collect();
            keys.sort();
            eprintln!("❌  No entry point found. Define a class 'Main' with a method 'main'.");
            eprintln!("    Available methods: {:?}", keys);
            process::exit(1);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// omni check <file.omni>
// ─────────────────────────────────────────────────────────────────────────────
fn cmd_check(args: &[String]) {
    let path = require_file_arg(args, "check");
    let source = read_source(&path);

    match compile(&source) {
        Ok(program) => {
            let class_count  = program.classes.len();
            let method_count: usize = program.classes.iter()
                .map(|c| c.members.iter().filter(|m| {
                    matches!(m, omni_compiler::ast::ClassMember::Method(_, _))
                }).count())
                .sum();

            println!("✅  '{}' — OK", path);
            println!("    {} class(es), {} method(s) found.", class_count, method_count);
        }
        Err(e) => {
            eprintln!("❌  '{}' — Errors found:", path);
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Returns the first suitable entry point from the compiled program.
/// Priority: "Main::main" → any method ending in "::main" → first available.
fn find_entry(compiled: &omni_compiler::bytecode::CompiledProgram) -> Option<String> {
    if compiled.methods.contains_key("Main::main") {
        return Some("Main::main".to_string());
    }
    if let Some(k) = compiled.methods.keys().find(|k| k.ends_with("::main")) {
        return Some(k.clone());
    }
    compiled.methods.keys().next().cloned()
}

/// Read a `.omni` source file from disk.
fn read_source(path: &str) -> String {
    if !path.ends_with(".omni") {
        eprintln!("⚠️   Warning: '{}' does not have a .omni extension.", path);
    }
    match fs::read_to_string(path) {
        Ok(src) => src,
        Err(e)  => {
            eprintln!("❌  Cannot read '{}': {}", path, e);
            process::exit(1);
        }
    }
}

/// Extract the required file path argument, or print usage and exit.
fn require_file_arg<'a>(args: &'a [String], cmd: &str) -> String {
    match args.get(2) {
        Some(p) => p.clone(),
        None => {
            eprintln!("❌  Usage: omni {} <file.omni>", cmd);
            process::exit(1);
        }
    }
}

fn print_help() {
    println!(
r#"
╔══════════════════════════════════════════╗
║        Omni Language Toolchain           ║
╚══════════════════════════════════════════╝

USAGE:
  omni <command> <file.omni>

COMMANDS:
  run   <file.omni>   Compile and execute an Omni source file.
  check <file.omni>   Type-check only — prints errors without running.
  help                Show this help message.

EXAMPLES:
  omni run   hello.omni
  omni check student.omni

ENTRY POINT:
  The VM looks for a class called 'Main' with a method 'main':

    class Main {{
        public function main() {{
            print("Hello, Omni!");
        }}
    }}

SOURCE FILES:
  Omni source files use the .omni extension.
  Naming rules:
    - Classes must start with a Capital letter  (e.g.  Student, Main)
    - Variables must start with a lowercase letter (e.g. name, total)
    - The keyword 'goto' is forbidden.
"#
    );
}
