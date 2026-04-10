// omni-vm/src/lib.rs
// Phase 5: Omni Virtual Machine — public API

pub mod gc;
pub mod vm;

// ════════════════════════════════════════════════════════════════════════════
// Phase 4 & 5 Unit Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use omni_compiler::bytecode::{Chunk, CompiledProgram, Instruction};
    use crate::gc::{GarbageCollector, Value};
    use crate::vm::Vm;

    // ── Phase 4: Bytecode / Codegen tests ────────────────────────────────

    /// Verify code generation for the Student whitepaper example produces
    /// a non-empty method chunk for each declared method.
    #[test]
    fn test_codegen_student_class_methods_exist() {
        use omni_compiler::codegen::CodeGen;

        let src = r#"
            class Student {
                private var name : String ;
                private var grade : Int ;
                public Student(in n : String, in g : Int) { name = n; grade = g; }
                public function getGrade() : Int { return grade; }
                public function describe() { print(name); }
            }
        "#;
        let program = omni_compiler::compile(src).expect("compile failed");
        let mut gen = CodeGen::new();
        gen.generate(&program);

        assert!(gen.output.methods.contains_key("Student::Student"),
            "Constructor chunk missing");
        assert!(gen.output.methods.contains_key("Student::getGrade"),
            "getGrade chunk missing");
        assert!(gen.output.methods.contains_key("Student::describe"),
            "describe chunk missing");
    }

    /// Verify if-else generates correct JumpIfFalse / Jump instructions.
    #[test]
    fn test_codegen_if_else_jumps() {
        use omni_compiler::codegen::CodeGen;

        let src = r#"
            class Logic {
                public function check(in x : Int) {
                    if (x == 0) {
                        print("zero");
                    } else {
                        print("nonzero");
                    }
                }
            }
        "#;
        let program = omni_compiler::compile(src).expect("compile failed");
        let mut gen = CodeGen::new();
        gen.generate(&program);

        let chunk = gen.output.methods.get("Logic::check").expect("chunk missing");
        // There must be at least one JumpIfFalse and one Jump for the if-else.
        let has_jump_if_false = chunk.code.iter().any(|i| matches!(i, Instruction::JumpIfFalse(_)));
        let has_jump = chunk.code.iter().any(|i| matches!(i, Instruction::Jump(_)));
        assert!(has_jump_if_false, "Missing JumpIfFalse for if condition");
        assert!(has_jump, "Missing Jump for if-else");
    }

    /// Verify foreach emits ForallBegin / ForallEnd markers.
    #[test]
    fn test_codegen_foreach_markers() {
        use omni_compiler::codegen::CodeGen;

        let src = r#"
            class Loop {
                public function run(in items : List) {
                    foreach (x in items) { print(x); }
                }
            }
        "#;
        let program = omni_compiler::compile(src).expect("compile failed");
        let mut gen = CodeGen::new();
        gen.generate(&program);

        let chunk = gen.output.methods.get("Loop::run").expect("chunk missing");
        assert!(chunk.code.iter().any(|i| matches!(i, Instruction::ForallBegin { .. })),
            "Missing ForallBegin");
        assert!(chunk.code.iter().any(|i| matches!(i, Instruction::ForallEnd)),
            "Missing ForallEnd");
    }

    /// Verify try-catch emits TryBegin / TryEnd instructions.
    #[test]
    fn test_codegen_try_catch_instructions() {
        use omni_compiler::codegen::CodeGen;

        let src = r#"
            class Safe {
                public function run() throws IoException {
                    try {
                        doThing();
                    } catch (IoException e) {
                        log("caught");
                    }
                }
            }
        "#;
        let program = omni_compiler::compile(src).expect("compile failed");
        let mut gen = CodeGen::new();
        gen.generate(&program);

        let chunk = gen.output.methods.get("Safe::run").expect("chunk missing");
        assert!(chunk.code.iter().any(|i| matches!(i, Instruction::TryBegin { .. })),
            "Missing TryBegin");
        assert!(chunk.code.iter().any(|i| matches!(i, Instruction::TryEnd { .. })),
            "Missing TryEnd");
    }

    /// Verify closure generates a separate entry in the methods map.
    #[test]
    fn test_codegen_closure_compiled_separately() {
        use omni_compiler::codegen::CodeGen;

        let src = r#"
            class Adder {
                public function makeAdder(in x : Int) {
                    return function(y : Int) { return x + y; };
                }
            }
        "#;
        let program = omni_compiler::compile(src).expect("compile failed");
        let mut gen = CodeGen::new();
        gen.generate(&program);

        // The closure should be stored as a separate chunk with "closure" in the key.
        let has_closure = gen.output.methods.keys().any(|k| k.contains("closure"));
        assert!(has_closure, "Closure should be compiled as a separate chunk");
    }

    // ── Phase 5: GC Tests ─────────────────────────────────────────────────

    /// Basic allocation and live count tracking.
    #[test]
    fn test_gc_allocation_live_count() {
        let mut gc = GarbageCollector::new();
        assert_eq!(gc.live_count(), 0);
        gc.allocate("Student");
        gc.allocate("Student");
        gc.allocate("Student");
        assert_eq!(gc.live_count(), 3);
    }

    /// Objects with no roots must be swept after a full mark+sweep cycle.
    #[test]
    fn test_gc_unreachable_object_swept() {
        let mut gc = GarbageCollector::new();
        gc.allocate("Ghost");   // allocated but never referenced from roots
        assert_eq!(gc.live_count(), 1);

        // Mark with empty root set — Ghost has no roots.
        gc.mark_roots(&[]);
        while !gc.mark_step() {}
        gc.sweep();

        assert_eq!(gc.live_count(), 0, "Unreachable Ghost object should be swept");
    }

    /// A rooted object must survive a GC cycle.
    #[test]
    fn test_gc_rooted_object_survives() {
        let mut gc = GarbageCollector::new();
        let handle = gc.allocate("Student");

        // Mark with the live root.
        gc.mark_roots(&[handle]);
        while !gc.mark_step() {}
        gc.sweep();

        assert_eq!(gc.live_count(), 1, "Rooted Student should survive sweep");
    }

    /// After a sweep, freed slots must be reused on the next allocation.
    #[test]
    fn test_gc_free_slot_reuse() {
        let mut gc = GarbageCollector::new();
        let _ = gc.allocate("Temp");  // will be swept

        gc.mark_roots(&[]);
        while !gc.mark_step() {}
        gc.sweep();

        // Next allocation should reuse the freed slot.
        let new_handle = gc.allocate("Reused");
        assert_eq!(new_handle, 0, "Freed slot should be reused");
        assert_eq!(gc.heap_size(), 1, "Heap should not grow — slot was reused");
    }

    /// Object field set and get round-trip.
    #[test]
    fn test_gc_field_access() {
        let mut gc = GarbageCollector::new();
        let h = gc.allocate("Person");
        gc.get_mut(h).unwrap().fields.insert("name".to_string(), Value::Str("Alice".to_string()));
        let name = gc.get(h).unwrap().fields.get("name").cloned().unwrap();
        assert!(matches!(name, Value::Str(s) if s == "Alice"));
    }

    // ── Phase 5: VM execution tests ───────────────────────────────────────

    /// A manually crafted chunk: push 10, push 5, AddInt, ReturnValue → Int(15).
    #[test]
    fn test_vm_add_two_integers() {
        let mut chunk = Chunk::new();
        chunk.emit(Instruction::PushInt(10), 1);
        chunk.emit(Instruction::PushInt(5), 1);
        chunk.emit(Instruction::AddInt, 1);
        chunk.emit(Instruction::ReturnValue, 1);

        let mut program = CompiledProgram::default();
        program.methods.insert("Main::main".to_string(), chunk);

        let mut vm = Vm::new(program);
        let result = vm.execute("Main::main").expect("VM error");
        assert!(matches!(result, Some(Value::Int(15))));
    }

    /// Branching: push false, JumpIfFalse → push 99, ReturnValue → Int(99).
    #[test]
    fn test_vm_jump_if_false_taken() {
        let mut chunk = Chunk::new();
        chunk.emit(Instruction::PushBool(false), 1);
        chunk.emit(Instruction::JumpIfFalse(3), 1);   // jumps to index 3
        chunk.emit(Instruction::PushInt(0), 1);        // index 2: skipped
        chunk.emit(Instruction::PushInt(99), 1);       // index 3: always reached
        chunk.emit(Instruction::ReturnValue, 1);

        let mut program = CompiledProgram::default();
        program.methods.insert("Main::main".to_string(), chunk);

        let mut vm = Vm::new(program);
        let result = vm.execute("Main::main").expect("VM error");
        assert!(matches!(result, Some(Value::Int(99))));
    }

    /// Division by zero returns VmError::DivisionByZero.
    #[test]
    fn test_vm_division_by_zero() {
        let mut chunk = Chunk::new();
        chunk.emit(Instruction::PushInt(10), 1);
        chunk.emit(Instruction::PushInt(0), 1);
        chunk.emit(Instruction::DivInt, 1);
        chunk.emit(Instruction::ReturnValue, 1);

        let mut program = CompiledProgram::default();
        program.methods.insert("Main::main".to_string(), chunk);

        let mut vm = Vm::new(program);
        assert!(matches!(vm.execute("Main::main"), Err(crate::vm::VmError::DivisionByZero)));
    }

    /// GC integration: allocate 200 objects so the threshold triggers automatically.
    #[test]
    fn test_vm_gc_triggers_at_threshold() {
        let mut chunk = Chunk::new();
        // Allocate Student 200 times, discarding each with Pop.
        let class_idx = chunk.intern_name("Ghost");
        for _ in 0..200 {
            chunk.emit(Instruction::New { class_idx, argc: 0 }, 1);
            chunk.emit(Instruction::Pop, 1);
        }
        chunk.emit(Instruction::PushInt(1), 1);
        chunk.emit(Instruction::ReturnValue, 1);

        let mut program = CompiledProgram::default();
        program.methods.insert("Main::main".to_string(), chunk);

        let mut vm = Vm::new(program);
        let result = vm.execute("Main::main");
        // Should complete without panic — GC must have triggered.
        assert!(result.is_ok(), "VM crashed: {:?}", result);
    }
}
