# Omni Language Guided Introduction: From 0 to Pro

Welcome to Omni, the programming language designed for **Performance**, **Concurrency**, and **Safety**. This guide takes you from your first "Hello Omni" to building high-performance, parallel applications with robust exception handling.

---

## 🚀 1. Getting Started: The Omni Zen

Omni is a **Nominal**, **Statically-Typed** (with inference), **Object-Oriented** language.
- **Safety**: Null-safe by default. Use `?` for options.
- **Speed**: Compiled to compact bytecode for a high-performance VM.
- **Simplicity**: No complex borrow checkers, just a fast Incremental GC.

### Your First Program
Create a file `hello.omni`:
```omni
class Main {
    public function main() {
        print("Hello, Omni World!");
    }
}
```
**Run it**: `omni run hello.omni`

> [!IMPORTANT]
> **Prototype Requirement**: In the current Omni prototype, all code must be encapsulated within a class. Top-level functions or script-style statements are not yet supported. Every executable program must have a `Main` class with a `public function main()` method.

---

## 💎 2. Core Syntax & Data Types

Omni keywords are modern and expressive.

### Variables & Types
Variables must be declared within a method or as class fields.
```omni
class Example {
    private var score : Float = 3.14; // Field

    public function showcase() {
        var count = 42;             // Inferred as Int
        var name : String = "Omni"; // Explicit type
        var isReady : Bool = true;
        var maybeAge : Int? = null; // Optional type (Null-Safe)
    }
}
```

### Control Flow
```omni
public function logic(in count : Int, in items : List<String>) {
    if (count > 0) {
        print("Positive");
    } else {
        print("Zero or negative");
    }

    foreach (item in items) {
        print(item);
    }
}
```

---

## ⚡ 3. Objects & Classes

Everything in Omni is an object. Classes support fields, methods, and constructors.

```omni
class Student {
    private var name : String;
    private var grade : Int;

    public Student(in n : String, in g : Int) {
        name = n;
        grade = g;
    }

    public function describe() {
        print(name + " has grade " + grade);
    }
}

// Usage in a Main class
class Main {
    public function main() {
        var classList = new List<Student>();
        
        // Best practice: instantiate separately
        var s1 = new Student("Alice", 95);
        classList.add(s1);
        
        s1.describe();
    }
}
```

---

## 🛡️ 4. Robust Exception Handling (New!)

Omni features a sophisticated stack-unwinding exception system with guaranteed `finally` execution.

```omni
class ErrorHandler {
    public function demo() {
        try {
            throw new NetworkException("Timeout");
        } catch (NetworkException e) {
            print("Caught: " + e.getMessage());
        } finally {
            print("This always runs, even if you return or throw!");
        }
    }
}
```

### Return from Try
Omni ensures that if you `return` from a `try` block, the `finally` block executes *before* the function actually returns.

---

## 🏎️ 5. Concurrency: The Professional Level

Omni is built for the multicore era.

### Parallel Loops (`forall`)
Process collections in parallel with zero boilerplate.
```omni
class ParallelProcessor {
    public function process() {
        forall (i = 0 to 1000) {
            doWork(i); // Efficiently distributed across CPU cores
        }
    }
}
```

### Monitors (Thread Safety)
Protect shared data using the `monitor` keyword.
```omni
class Synchronization {
    public function safeUpdate(sharedObject : Counter) {
        monitor (sharedObject) {
            sharedObject.increment(); // Thread-safe execution
        }
    }
}
```

---

> [!CAUTION]
> ### ⚠️ Current Prototype Limitations
> - **No Method Overloading**: Methods are resolved by name only. You cannot have multiple methods with the same name and different parameter types.
> - **Built-in Generics Only**: While `List<T>` is supported, **user-defined generics** (e.g., `class MyBox<T>`) are not yet implemented.
> - **Strict Nominal Typing**: Two classes with identical fields are NOT interchangeable; they must be the exact same named type.
> - **Limited Primitive Types**: Only `Int`, `Float`, `Bool`, and `String` are supported. There are no `Char`, `Byte`, or `Short` types.
> - **No Static Members**: All fields and methods are instance-level.
> - **Strict Semicolons**: Unlike some modern languages, Omni requires a semicolon after *every* field declaration and statement.
> - **Inheritance Caveats**: Deep inheritance hierarchies and complex `super` call patterns are still experimental in the VM.

## 🛠️ 6. IDE Support

To get syntax highlighting and Omni-aware features in VS Code:
1. Locate the `omni-vscode/omni-lang-1.0.0.vsix` file.
2. Run: `code --install-extension omni-lang-1.0.0.vsix`

---

## 🎓 Pro Tips
- Use `in` parameters for read-only views. Note that `in` parameters may prevent calling methods that modify (or are perceived to modify) the object, such as `List.add()`.
- **`in` Mode Propagation**: If you pass an `in` parameter as an argument to another method, that receiving method must also declare the parameter as `in`.
- **Built-in Collections**: `List<T>` is built-in to the compiler and VM. You do **not** need to import `stdlib.omni` to use it.
- **VM Best Practice**: In the current prototype, avoid nesting `new` calls directly inside method arguments (e.g., `list.add(new Student(...))`). Instead, instantiate objects as separate variables before passing them.
- Checked exceptions must be declared in method signatures: `function run() throws MyException`.
- String concatenation works naturally with `+`.

---

# 🔧 Maintenance & Developer Workflow

This section is for developers maintaining the Omni toolchain itself.

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
