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

---

## 💎 2. Core Syntax & Data Types

Omni keywords are modern and expressive.

### Variables & Types
```omni
var count = 42;                 // Inferred as Int
var name : String = "Omni";     // Explicit type
var score : Float = 3.14;
var isReady : Bool = true;
var maybeAge : Int? = null;     // Optional type (Null-Safe)
```

### Control Flow
```omni
if (count > 0) {
    print("Positive");
} else {
    print("Zero or negative");
}

foreach (item in items) {
    print(item);
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

// Usage
var s = new Student("Alice", 95);
s.describe();
```

---

## 🛡️ 4. Robust Exception Handling (New!)

Omni features a sophisticated stack-unwinding exception system with guaranteed `finally` execution.

```omni
try {
    throw new NetworkException("Timeout");
} catch (NetworkException e) {
    print("Caught: " + e.getMessage());
} finally {
    print("This always runs, even if you return or throw!");
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
forall (i = 0 to 1000) {
    doWork(i); // Efficiently distributed across CPU cores
}
```

### Monitors (Thread Safety)
Protect shared data using the `monitor` keyword.
```omni
monitor (sharedObject) {
    sharedObject.increment(); // Thread-safe execution
}
```

---

## 🛠️ 6. IDE Support

To get syntax highlighting and Omni-aware features in VS Code:
1. Locate the `omni-vscode/omni-lang-1.0.0.vsix` file.
2. Run: `code --install-extension omni-lang-1.0.0.vsix`

---

## 🎓 Pro Tips
- Use `in` parameters for read-only views of data.
- Checked exceptions must be declared in method signatures: `function run() throws MyException`.
- String concatenation works naturally with `+`.

**Congratulations! You are now an Omni Developer.**
