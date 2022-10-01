use btreevec::CursorMut;

fn main() {}

pub fn bar<'c, 'a>(x: CursorMut<'c, &'static str, 3>) -> CursorMut<'c, &'a str, 3> {
    x
}
