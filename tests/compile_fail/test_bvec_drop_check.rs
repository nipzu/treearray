fn main() {
    let mut x = btreevec::BTreeVec::<&str>::new();
    {
        let s = String::from("Hello!");
        x.push_back(s.as_str());
    }
}