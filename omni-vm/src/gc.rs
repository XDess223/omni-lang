// omni-vm/src/gc.rs
// Phase 5: Incremental Mark-Sweep Garbage Collector
//
// Design decisions (per Omni whitepaper):
//   - Implicit: programmer never calls free/delete.
//   - Incremental: the collector does work in small fixed-size "slices"
//     instead of one giant stop-the-world pause. Each slice marks a
//     bounded number of objects then yields, keeping the UI/application
//     responsive.
//   - Mark phase: trace all live objects by following roots → pointers.
//   - Sweep phase: reclaim any object whose mark bit is still false.
//   - No dangling pointers: the GC holds sole ownership of all heap objects.

use std::collections::HashMap;

// ── Heap Value ────────────────────────────────────────────────────────────────

/// The runtime value of any Omni object slot.
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Null,
    /// A heap-allocated object identified by its GC handle.
    Object(HeapHandle),
    /// A compiled closure identified by its chunk key.
    Closure(String),
}

/// A u32 index into the GC's heap arena.
pub type HeapHandle = u32;

/// A heap-allocated Omni object (class instance).
#[derive(Debug)]
pub struct HeapObject {
    /// The class this instance belongs to.
    pub class_name: String,
    /// Named fields.
    pub fields: HashMap<String, Value>,
    /// GC mark bit — set to `true` during the mark phase, cleared before each cycle.
    pub marked: bool,
}

// ── Garbage Collector ─────────────────────────────────────────────────────────

/// Incremental mark-sweep GC for the Omni VM.
pub struct GarbageCollector {
    /// The heap arena — all live and dead objects live here.
    heap: Vec<Option<HeapObject>>,
    /// Free-list: indices of slots that have been swept and can be reused.
    free_list: Vec<HeapHandle>,
    /// Total number of live allocations since last sweep.
    live_count: usize,
    /// Trigger a GC cycle after this many allocations.
    gc_threshold: usize,

    // ── Incremental state ─────────────────────────────────────────────────
    /// Objects still to be marked in the current incremental cycle.
    mark_worklist: Vec<HeapHandle>,
    /// True when an incremental mark cycle is in progress.
    marking_in_progress: bool,
    /// Number of objects to mark per incremental step (slice size).
    mark_slice_size: usize,
}

impl GarbageCollector {
    pub fn new() -> Self {
        Self {
            heap: Vec::new(),
            free_list: Vec::new(),
            live_count: 0,
            gc_threshold: 128,
            mark_worklist: Vec::new(),
            marking_in_progress: false,
            mark_slice_size: 32,
        }
    }

    // ── Allocation ────────────────────────────────────────────────────────

    /// Allocate a new heap object and return its handle.
    pub fn allocate(&mut self, class_name: &str) -> HeapHandle {
        self.live_count += 1;

        let obj = HeapObject {
            class_name: class_name.to_string(),
            fields: HashMap::new(),
            marked: false,
        };

        if let Some(slot) = self.free_list.pop() {
            self.heap[slot as usize] = Some(obj);
            slot
        } else {
            let handle = self.heap.len() as HeapHandle;
            self.heap.push(Some(obj));
            handle
        }
    }

    /// Access an object by handle (panics on invalid handle — use carefully).
    pub fn get(&self, handle: HeapHandle) -> Option<&HeapObject> {
        self.heap.get(handle as usize)?.as_ref()
    }

    /// Mutably access an object by handle.
    pub fn get_mut(&mut self, handle: HeapHandle) -> Option<&mut HeapObject> {
        self.heap.get_mut(handle as usize)?.as_mut()
    }

    // ── Incremental Mark Phase ────────────────────────────────────────────

    /// Seed the mark phase with a set of root handles (stack values, globals).
    /// Call this once before beginning incremental steps.
    pub fn mark_roots(&mut self, roots: &[HeapHandle]) {
        for &handle in roots {
            self.mark_worklist.push(handle);
        }
        self.marking_in_progress = true;
    }

    /// Perform one incremental marking step — processes up to `mark_slice_size`
    /// objects. Returns `true` when the full mark phase is complete.
    pub fn mark_step(&mut self) -> bool {
        if !self.marking_in_progress {
            return true;
        }

        let batch: Vec<HeapHandle> = self.mark_worklist
            .drain(..self.mark_worklist.len().min(self.mark_slice_size))
            .collect();

        for handle in batch {
            if let Some(obj) = self.heap.get_mut(handle as usize).and_then(|s| s.as_mut()) {
                if obj.marked { continue; } // already traced
                obj.marked = true;

                // Gray all referenced child objects (follow field pointers).
                let children: Vec<HeapHandle> = obj.fields.values()
                    .filter_map(|v| if let Value::Object(h) = v { Some(*h) } else { None })
                    .collect();
                self.mark_worklist.extend(children);
            }
        }

        if self.mark_worklist.is_empty() {
            self.marking_in_progress = false;
            true // mark phase complete
        } else {
            false // more work to do
        }
    }

    // ── Sweep Phase ───────────────────────────────────────────────────────

    /// Sweep the heap: reclaim all objects that were not marked.
    /// Call only after `mark_step` returns `true`.
    pub fn sweep(&mut self) {
        let mut reclaimed = 0;
        for (idx, slot) in self.heap.iter_mut().enumerate() {
            if let Some(obj) = slot {
                if !obj.marked {
                    // Dead object — reclaim its slot.
                    *slot = None;
                    self.free_list.push(idx as HeapHandle);
                    reclaimed += 1;
                } else {
                    // Live object — clear mark bit for the next cycle.
                    obj.marked = false;
                }
            }
        }
        self.live_count = self.live_count.saturating_sub(reclaimed);
    }

    /// Returns true when an incremental GC cycle should begin
    /// (live object count exceeds the adaptive threshold).
    pub fn should_collect(&self) -> bool {
        self.live_count >= self.gc_threshold
    }

    /// Live object count (useful for diagnostics and tests).
    pub fn live_count(&self) -> usize {
        self.live_count
    }

    /// Total heap slots (including free ones).
    pub fn heap_size(&self) -> usize {
        self.heap.len()
    }
}
