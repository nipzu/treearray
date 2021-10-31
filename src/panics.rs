#[cold]
#[track_caller]
pub fn panic_out_of_bounds(index: usize, len: usize) -> ! {
    panic!(
        "index out of bounds: the len is {} but the index is {}",
        len, index
    );
}

#[cold]
#[track_caller]
pub fn panic_length_overflow() -> ! {
    panic!("length overflow");
}
