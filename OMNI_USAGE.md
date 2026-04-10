# 🚀 Omni Language: From 0 to Pro

Omni is a state-of-the-art programming language built for **safety**, **concurrency**, and **performance**. This guide will take you from your first "Hello World" to architecting high-performance parallel systems.

---

## 🟢 Level 0: The Basics

### 1. Hello World
The entry point is a class named `Main` with a `main` method.

```omni
class Main {
    public function main() {
        print("Hello, Omni!");
    }
}
```

### 2. Variables & Types
Omni has a strong, nominal type system.

```omni
var x : Int = 42;
var pi : Float = 3.14;
var name = "Omni"; // Type inference handles this
```

---

## 🟡 Level 1: Reliability & Safety

### 3. Null Safety
Omni is null-safe by default. You cannot assign `null` to a standard type. Use `?` for optionals.

```omni
var safe : String = null;    // ❌ Error!
var maybe : String? = null;  // ✅ OK
```

### 4. Method Modes (`in`)
Protect your objects from accidental mutation. `in` parameters are read-only views.

```omni
public function show(in user: User) {
    user.name = "New"; // ❌ Error: 'in' parameters are immutable!
}
```

---

## 🟠 Level 2: Modern Patterns

### 5. Generics
Write reusable code with `List<T>`.

```omni
var list = new List<String>();
list.add("First");
var item : String = list.get(0);
```

### 6. Higher-Order Functions
Functions are first-class. Pass them around easily.

```omni
public function runTwice(in task: () -> Void) {
    task();
    task();
}

runTwice(() => print("Done!"));
```

---

## 🔴 Level 3: Professional Concurrency

### 7. Statements-Level Parallelism (`forall`)
Don't just loop — parallelize. `forall` runs iterations across all available CPU cores.

```omni
forall (i = 0 to items.size()) {
    processLargeDataset(items.get(i));
}
```

### 8. Synchronization (`monitor`)
Safe shared state is achieved via `monitor` blocks. They are ultra-lightweight and lock-free where possible.

```omni
class Counter {
    private var count = 0;
    public function inc() {
        monitor(this) {
            count = count + 1;
        }
    }
}
```

---

## ⚙️ The Toolchain

### Installing
```powershell
cargo install --path omni-cli --locked
```

### Running
```powershell
omni run your_file.omni
```

### Checking (Static Analysis)
```powershell
omni check your_file.omni
```

---

## 🛡️ Omni Philosophy
1. **Safety First**: Null-safe, Memory-safe, Thread-safe.
2. **Readability**: Nominal types and explicit modes make code self-documenting.
3. **Power**: High-level concurrency with low-level VM performance.
