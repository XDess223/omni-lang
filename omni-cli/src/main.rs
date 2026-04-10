// omni-cli/src/main.rs
// The `omni` command-line tool.

use std::{env, fs, process, time::Instant};
use omni_compiler::{compile, codegen::CodeGen};
use omni_vm::vm::Vm;

fn main() {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(String::as_str) {
        Some("run")   => cmd_run(&args),
        Some("check") => cmd_check(&args),
        Some("help") | Some("--help") | Some("-h") => print_help(),
        _ => {
            eprintln!("\x1b[1;31mError:\x1b[0m Unknown or missing sub-command.");
            eprintln!("Run `omni help` for usage.");
            process::exit(1);
        }
    }
}

fn cmd_run(args: &[String]) {
    let path = require_file_arg(args, "run");
    let start_time = Instant::now();

    println!("\x1b[1;34m╔══════════════════════════════════════════╗\x1b[0m");
    println!("\x1b[1;34m║        Omni Execution Pipeline           ║\x1b[0m");
    println!("\x1b[1;34m╚══════════════════════════════════════════╝\x1b[0m");

    let source = read_source(&path);

    // ── Stage 1: Semantic Analysis ────────────────────────────────────────
    print!("  \x1b[1;36m[1/3]\x1b[0m Analyzing symbols & safety... ");
    let program = match compile(&source) {
        Ok(p)  => { println!("\x1b[32mOK\x1b[0m"); p },
        Err(e) => {
            println!("\x1b[31mFAILED\x1b[0m");
            eprintln!("\n\x1b[1;31m❌ Semantic Error:\x1b[0m\n{}", e);
            process::exit(1);
        }
    };

    // ── Stage 2: Bytecode Generation ──────────────────────────────────────
    print!("  \x1b[1;36m[2/3]\x1b[0m Generating bytecode... ");
    let mut gen = CodeGen::new();
    gen.generate(&program);
    let compiled = gen.output;
    println!("\x1b[32mOK\x1b[0m");

    // ── Stage 3: VM Initialization & Execution ────────────────────────────
    let entry = find_entry(&compiled);
    match entry {
        Some(key) => {
            println!("  \x1b[1;36m[3/3]\x1b[0m Invoking \x1b[1m{}\x1b[0m...\n", key);
            let mut vm = Vm::new(compiled);
            
            // Background thread setup (if any)
            if vm.thread_id == 0 {
                // Main thread indicator
            }

            match vm.execute(&key) {
                Ok(result) => {
                    let elapsed = start_time.elapsed();
                    println!("\n\x1b[1;32m✅ Program completed successfully in {:?}\x1b[0m", elapsed);
                    if let Some(val) = result {
                        println!("Result: \x1b[33m{:?}\x1b[0m", val);
                    }
                }
                Err(e) => {
                    eprintln!("\n\x1b[1;31m❌ Runtime Error: {:?}\x1b[0m", e);
                    process::exit(1);
                }
            }
        }
        None => {
            eprintln!("\x1b[1;31m❌ Error:\x1b[0m No entry point found. Add `class Main {{ public function main() ... }}`");
            process::exit(1);
        }
    }
}

fn cmd_check(args: &[String]) {
    let path = require_file_arg(args, "check");
    let source = read_source(&path);

    println!("\x1b[1;34mOmni Static Analysis Check\x1b[0m");
    match compile(&source) {
        Ok(program) => {
            let class_count = program.classes.len();
            println!("\x1b[32m✅ Analysis Passed: '{}'\x1b[0m", path);
            println!("   Found {} class(es).", class_count);
        }
        Err(e) => {
            eprintln!("\x1b[31m❌ Errors found in '{}':\x1b[0m", path);
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}

fn find_entry(compiled: &omni_compiler::bytecode::CompiledProgram) -> Option<String> {
    if compiled.methods.contains_key("Main::main") {
        Some("Main::main".to_string())
    } else if let Some(k) = compiled.methods.keys().find(|k| k.ends_with("::main")) {
        Some(k.clone())
    } else {
        compiled.methods.keys().next().cloned()
    }
}

fn read_source(path: &str) -> String {
    match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("\x1b[31m❌ File Error:\x1b[0m Cannot read '{}': {}", path, e);
            process::exit(1);
        }
    }
}

fn require_file_arg(args: &[String], cmd: &str) -> String {
    match args.get(2) {
        Some(p) => p.clone(),
        None => {
            println!("Usage: omni {} <file.omni>", cmd);
            process::exit(1);
        }
    }
}

fn print_help() {
    println!(
        r#"
\x1b[1;34m╔══════════════════════════════════════════╗
║        Omni Language Toolchain           ║
╚══════════════════════════════════════════╝\x1b[0m

\x1b[1mUSAGE:\x1b[0m
  omni <command> <file.omni>

\x1b[1mCOMMANDS:\x1b[0m
  run   <file.omni>   Compile and execute in the parallel VM.
  check <file.omni>   Perform full semantic validation.
  help                Show this message.

\x1b[1mPHILOSOPHY:\x1b[0m
  Omni is built for \x1b[36mSafety\x1b[0m, \x1b[36mConcurrency\x1b[0m, and \x1b[36mSpeed\x1b[0m.
  - Nominal Typing
  - Null-Safe by Default
  - Thread-local GC with Atomic Monitors
"#
    );
}
