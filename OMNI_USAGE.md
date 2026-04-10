# Omni Programming Language — Usage Guide

Omni is a high-performance, hybrid programming language designed for reliability, concurrency, and safety. It features a strict nominal type system, built-in null safety, and an incremental mark-sweep garbage collector.

---

## 🚀 Getting Started

Omni is implemented as a Rust-based toolchain. To use the compiler and virtual machine, ensure you have [Rust](https://www.rust-lang.org/) installed.

### Project Structure
- `omni-compiler`: Lexer, Parser, Semantic Analyzer, and Bytecode Generator.
- `omni-vm`: Stack-based Virtual Machine and Incremental Garbage Collector.

### Running Tests (Development)
To verify the installation and see the compiler/VM in action:
```powershell
# Run all tests in the workspace
cargo test --workspace
```

## 💻 Command Line Interface (CLI)

Omni comes with a built-in CLI tool. To use it globally, install it via Cargo:

```powershell
cargo install --path omni-cli --locked
```

### CLI Commands

```powershell
# Type-check a file (no execution)
omni check my_program.omni

# Compile and run a file
omni run my_program.omni

# Show help
omni help
```

---

## 🛠️ Language Syntax

### 1. Class Definitions
Classes are the primary unit of organization. They support inheritance and interface implementation.

```omni
class Student extends Person implements IPrintable {
    private var name : String ;
    private var grade : Int ;

    // Constructor
    public Student(in n : String, in g : Int) {
        name = n;
        grade = g;
    }

    public function getGrade() : Int {
        return grade;
    }
}
```

### 2. Variables and Type Inference
You can explicitly declare types or let the compiler infer them.

```omni
var x : Int = 10;      // Explicit
var y = 20;            // Inferred as Int
var msg = "Hello";     // Inferred as String
```

### 3. Null Safety
Omni is null-safe by default. Use the `?` modifier to allow a variable to hold `null`.

```omni
var name : String = null;     // ❌ COMPILE ERROR
var name : String ? = null;   // ✅ OK (Optional Type)
```

### 4. Method Modes (`in` parameters)
Parameters marked with `in` are read-only views. The compiler prevents any mutation of these objects within the method.

```omni
public function process(in data : Config) {
    data.value = 10; // ❌ COMPILE ERROR: Mutation of read-only parameter
}
```

### 5. Control Flow
Standard `if-else` and a powerful iterator-based `foreach`.

```omni
if (score > 90) {
    print("A");
} else {
    print("B");
}

foreach (item in collection) {
    item.process();
}
```

### 6. Exception Handling
Checked exceptions must be caught or declared in the method signature.

```omni
public function load() throws IoException {
    try {
        file.open();
    } catch (IoException e) {
        log("Failed to open file");
        throw e;
    } finally {
        cleanup();
    }
}
```

---

## 📦 Compilation & Execution Workflow

The compiler transforms Omni source code into **Omni Bytecode**, which is then executed by the VM.

### 1. Lexing & Parsing
The compiler ensures the code conforms to the Omni EBNF grammar and enforces naming conventions (Classes must be `Capitalized`, variables `lowercase`).

### 2. Semantic Analysis
Omni performs deep analysis to verify:
- Nominal type equivalence.
- Purity of `in` parameters.
- Exception handling completeness.

### 3. Virtual Machine (VM)
The VM uses a stack-based architecture.
- **Operand Stack**: For intermediate calculations.
- **Call Stack**: Managed via `CallFrame` structures.
- **Garbage Collection**: An **Incremental Mark-Sweep GC** runs in the background, minimizing "stop-the-world" pauses.

---

## 📝 Example: Hello World

The entry point for the Omni Virtual Machine is the `main` method of a class named `Main`.

**Create a file named `hello.omni`:**

```omni
class Main {
    public function main() {
        var greeting = "Hello, Omni!";
        print(greeting);
    }
}
```

**Run it via the CLI:**
```powershell
omni run hello.omni
```

**Expected Output:**
```
▶  Running 'Main::main' …

Hello, Omni!

✅  Program completed.
```

## 📚 Example: Student Grade Tracker

**Create a file named `student.omni`:**

```omni
class Student {
    private var name : String ;
    private var grade : Int ;

    public Student(in n : String, in g : Int) {
        name = n;
        grade = g;
    }

    public function getGrade() : Int {
        return grade;
    }

    public function describe() {
        print(name);
    }
}

class Main {
    public function main() {
        var student = new Student("Alice", 95);
        
        if (student.getGrade() >= 90) {
            print("Excellent result!");
        }

        student.describe();
    }
}
```

**Run it:**
```powershell
omni run student.omni
```

---

## 🛡️ Design Principles
- **No Goto**: Forbidden at the lexical level.
- **Nominal Equivalence**: Two shapes are only equal if their type *names* match.
- **Memory Safety**: No dangling pointers or manual memory management.
