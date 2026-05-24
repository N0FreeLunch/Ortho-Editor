use d2r_macros::rhythm_alignment;

// Case 1: missing width
#[rhythm_alignment(gap = "alpha_v0")]
struct Invalid1 {}

// Case 2: odd width
#[rhythm_alignment(width = 75)]
struct Invalid2 {}

fn main() {}
