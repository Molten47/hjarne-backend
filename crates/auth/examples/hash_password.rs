fn main() {
    let password = "AdminPassword123!";
    let hash = bcrypt::hash(password, bcrypt::DEFAULT_COST).unwrap();
    println!("{}", hash);
}