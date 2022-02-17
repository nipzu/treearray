fn main() {
    let _: &'static str = foo();
}

fn foo() -> &'static str {
    let mut bvec = btreevec::BTreeVec::<&'static str>::new();
    {
        let local = String::from("hello");
        let local_str: &str = &local;
        let mut cursor = bvec.cursor_at_mut(0);
        cursor.insert(local_str);
    }
    // This would be a 'static reference to `local`.
    // However, local has already been dropped, so this
    // would be Undefined Behavior
    bvec.get(0).unwrap()
}
