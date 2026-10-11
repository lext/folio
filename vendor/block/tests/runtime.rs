#![cfg(target_os = "macos")]

extern crate block;

use block::{ConcreteBlock, RcBlock};
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn stack_callback_preserves_arguments_and_captured_state() {
    let offset = 7;
    let callback = ConcreteBlock::new(|a: i32, b: i32| offset + a + b);
    // This callback only reads live, local values on the current thread.
    assert_eq!(unsafe { callback.call((13, 22)) }, 42);
}

struct Capture {
    value: String,
    drops: Rc<Cell<usize>>,
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

#[test]
fn native_copy_retains_callback_and_releases_capture_once() {
    fn make_callback(drops: Rc<Cell<usize>>) -> RcBlock<(usize,), usize> {
        let captured = Capture {
            value: "Folio".into(),
            drops,
        };
        ConcreteBlock::new(move |offset: usize| captured.value.len() + offset).copy()
    }

    let drops = Rc::new(Cell::new(0));
    let callback = make_callback(Rc::clone(&drops));
    // Heap copying goes through macOS's _Block_copy and validates the ISA.
    assert_eq!(drops.get(), 0);
    assert_eq!(unsafe { callback.call((2,)) }, 7);
    let retained = callback.clone();
    drop(callback);
    assert_eq!(drops.get(), 0);
    assert_eq!(unsafe { retained.call((3,)) }, 8);
    drop(retained);
    assert_eq!(drops.get(), 1);
}
