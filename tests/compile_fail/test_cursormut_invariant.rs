use btreevec::CursorMut;

fn main() {}

pub fn bar<'c, 'a>(x: CursorMut<'c, &'static str>) -> CursorMut<'c, &'a str> {
    x
}
