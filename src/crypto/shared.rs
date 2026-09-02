// order[i] is the rank of column i for columnar transposition.
#[must_use]
pub fn column_order(key: &str) -> Vec<usize> {
    let key_upper: Vec<char> = key.to_uppercase().chars().collect();
    let mut indices: Vec<usize> = (0..key_upper.len()).collect();
    indices.sort_by(|&a, &b| key_upper[a].cmp(&key_upper[b]).then(a.cmp(&b)));

    let mut order = vec![0; key_upper.len()];
    for (rank, &idx) in indices.iter().enumerate() {
        order[idx] = rank;
    }
    order
}
