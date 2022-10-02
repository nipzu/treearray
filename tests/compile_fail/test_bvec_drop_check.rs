fn main() {
    let mut x = bvec::BVec::<&str>::new();
    {
        let s = String::from("Hello!");
        x.push_back(s.as_str());
    }
}