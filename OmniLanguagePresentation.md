_Programming Language Design_

# O M N I

##### Programming Language

**Hybrid Imperative + Object-Oriented Architecture**


```
PARADIGM REVIEW
```
### Reference Programming Paradigm

Hybrid Imperative & Object-Oriented — Why Both?

###### Imperative

Modeled on von Neumann architecture (CPU + Memory)

Variables model memory cells

Assignments pipe data from CPU back to memory

Iteration via foreach & FORALL—most efficient for

sequential hardware

Direct mapping to hardware → maximum execution

efficiency

###### Object-Oriented

```
Abstract Data Types (Classes) hide internal data
representations
Encapsulation prevents accidental data corruption which
improve reliability
Single Inheritance + Interfaces avoids the C++ 'diamond
problem'
Dynamic Binding (Polymorphism) allows extensible
software design
Enable large-scale, safe software
```

```
LANGUAGE DESIGN
```
#### Reference Language Design

Formal Syntax via EBNF — the metalanguage that defines Omni's grammar

##### [ ] Optional Parts

```
Marks clauses that may be absent — e.g. the
else branch of an if statement.
```
##### { } Repetition

```
Zero-or-more occurrences — replaces
recursive BNF rules for lists.
```
##### ( | ) Alternatives

```
One-of-many choices — e.g. (* | / | %) for
arithmetic operators.
```
```
// if statement with optional else
<if_stmt> → if (<logic_expr>) <stmt> [else <stmt>]
```
```
// identifier list (zero or more repetitions)
<ident_list> → <identifier> {, <identifier>}
```
```
// arithmetic operator choice
<term> → <term> (* | / | %) <factor>
```
```
// foreach iteration statement
<iteration_stmt> → foreach ( <identifier> in <collection> )
<block> → '{' { <statement> } '}'
```

```
LANGUAGE DESIGN
```
#### Reference Language Design

Key Inspirations with Code Demonstrations

```
// Encapsulation (inspired by Java/C#)
class Person
private var name : String
private var age : Int
public function greet() {
print("Hello " + name);
}
}
```
```
// Closures (inspired by Python/Ruby)
function makeAdder(in x : Int) {
return function(y : Int) {
return x + y;
};
}
var add10 = makeAdder(10);
print(add10(5)); // → 15
```
**Java / C#** JIT compilation, generics, checked exceptions, namespaces, switch safety

**ML** Static typing + type inference — expressiveness without verbosity

**Ada** in-mode parameters (no side effects), Monitors for safe concurrency

**Python/Ruby** Readable syntax, keyword params, closures, primitive string types

**HPF** FORALL statement-level concurrency for multiprocessor hardware


```
OMNI DESIGN
```
#### Omni — Target Language Design

Preliminary Outline: Core Design Decisions & Features

**Type System**

```
Static typing + type inference (var x = 10)
Nominal type equivalence
Optional/nullable types → String? x
No implicit null — safety by default
```
**Memory & Safety**

```
Implicit garbage collection
Incremental mark-sweep (no stop-the-world)
Rectangular arrays with range checking
No dangling pointer risk
```
**Concurrency**

```
Monitors (protected objects) — auto-sync
FORALL statement-level parallelism
All RHS evaluated before assignment
Eliminates race conditions by design
```
**OOP & Generics**

```
Single inheritance + interfaces
Generics with bounded wildcards → List<T
extends Comparable>
Controlled reflection with security manifesto
Private-by-default encapsulation
```
**Syntax & Control**

```
No GOTO — forced structured programming
Switch/case without fall-through
Keyword parameters for subprograms
In-mode (read-only) function inputs
```
**Implementation**

```
Hybrid JIT — portability + performance
Source → Bytecode → Machine code
EBNF-defined grammar (recursive-descent)
Checked + unchecked exception system
```

```
OMNI DESIGN
```
#### Omni — EBNF Preliminary Grammar

Formal syntax rules covering classes, methods, iteration, and exception handling

// CLASS DEFINITION
<class_def> →
class <id> [extends <id>]
[implements <ident_list>]
'{' { <class_member> } '}'

<class_member> →
<access_mod> <method_def>
| <access_mod> <var_decl> ;

<access_mod> → public | private | protected

// METHOD DEFINITION
<method_def> → function <id>
( [<param_list>] ) [throws <ident_list>]
<block>

```
// STATEMENT TYPES
<statement> →
<assignment_stmt> ;
| <selection_stmt>
| <iteration_stmt>
| <method_call> ;
```
```
// FOREACH ITERATION
<iteration_stmt> →
foreach (<id> in <collection>)
<block>
```
```
// OPTIONAL TYPE DECLARATION
<var_decl> →
var <id> : <type> [?]
| var <id> = <expr> // inferred
```

```
OMNI CODE
```
#### Omni — Code Example

Student Grade Tracker — Encapsulation · Type Inference · in-mode · Generics · foreach

**class Student {**
private var name : String
private var grade : Int

public Student(in n : String, in g : Int) {
name = n; grade = g;
}

public function getGrade() : Int {
return grade;
}

public function describe() {
print("Student: " + name
+ ", Grade: " + grade);
}
}

```
// total inferred as Int via type inference
function getAverage(in students : List<Student>) : Int {
var total = 0;
foreach (s in students) {
total = total + s.getGrade();
}
return total / students.size();
}
```
```
var classList = List<Student>();
classList.add(new Student("Alice", 90));
classList.add(new Student("Bob", 75));
classList.add(new Student("Charlie", 85));
```
```
foreach (s in classList) { s.describe(); }
```
```
var avg = getAverage(classList);
print("Class Average: " + avg);
```
```
Encapsulation
private var hides data
```
```
in-mode Param
read-only inputs
```
```
Type Inference
var total = 0 → Int
```
```
Generics
List<Student>
```
```
foreach Iterator
safe, index-free loop
```

```
REFERENCES
```
#### References & Sources

###### 1

```
Concepts of Programming Languages — Robert W. Sebesta
Primary reference for language design criteria, BNF/EBNF notation, paradigm comparison, and implementation strategies. (12th Ed., Pearson)
https://www.pearson.com/en-us/subject-catalog/p/concepts-of-programming-languages/P
```
###### 2

**The Java Language Specification — Oracle**

```
Reference for JIT compilation model, generics, checked exception design, single-inheritance with interfaces. https://docs.oracle.com/javase/specs/
```
###### 3

**C# Language Reference — Microsoft Docs**

```
Inspiration for namespace design, safe switch/case fall-through prevention, and delegate-based closure patterns. https://learn.microsoft.com/en-
us/dotnet/csharp/language-reference/
```
###### 4

**The Definition of Standard ML — Milner, Tofte, Harper, MacQueen**

```
Foundation for Omni's static type inference system (Hindley-Milner style). MIT Press, 1997. https://mitpress.mit.edu/9780262631815/the-definition-of-standard-ml/
```
###### 5

```
Ada 2012 Reference Manual — ISO/IEC 8652
Basis for Omni's in-mode parameters, Monitor-based concurrency (protected objects), and reliability-first philosophy. https://www.ada-
auth.org/standards/rm12_w_tc1/html/RM-TTL.html
```
###### 6

**High Performance Fortran (HPF) Language Specification**

```
Inspiration for FORALL statement-level concurrency model and parallel array assignment semantics. https://www.netlib.org/hpf/hpf-v20.ps
```



