use happy_cracking::crypto::dh::{self, DhAction};

#[test]
fn test_dlog_dos_large_order() {
    // This test attempts to trigger OOM by providing a large prime order to Diffie-Hellman Dlog.
    // The BSGS algorithm attempts to allocate sqrt(order) entries.
    // Order = 2^61 - 1 (Mersenne prime), sqrt approx 2^30.5 (1.5 billion entries).

    let action = DhAction::Dlog {
        g: "2".to_string(),
        p: "1000000007".to_string(),
        target: "3".to_string(),
        order: "2305843009213693951".to_string(), // 2^61 - 1
    };

    let result = dh::run(action);

    // We expect an error.
    assert!(result.is_err(), "Expected an error due to OOM protection");
}
