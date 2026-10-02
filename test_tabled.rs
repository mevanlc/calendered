use tabled::{Table, settings::{Modify, Width, object::Segment}};

fn main() {
    let data = vec![("Hello", "World"), ("Test", "This is a very long string")];
    let mut table = Table::new(data);
    table.with(Modify::new(Segment::all()).with(Width::truncate(10)));
    println!("{}", table);
}
