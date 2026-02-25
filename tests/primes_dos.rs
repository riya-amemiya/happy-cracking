use happy_cracking::crypto::primes;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[test]
fn test_pollard_rho_dos_prevention() {
    // A semiprime that is difficult for Pollard's Rho without a limit.
    // p = next_prime(2^58), q = next_prime(2^59)
    // n = 166153499473114561646947067345511247
    let n = 166153499473114561646947067345511247u128;

    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let factors = primes::factorize(n);
        tx.send(factors).unwrap();
    });

    // Wait for 2 seconds. If it doesn't finish, it's considered a DoS/hang.
    // Without the fix, this will likely timeout.
    // With the fix, it should return quickly (either factored or failed).
    match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(_) => println!("Factorization completed successfully."),
        Err(_) => panic!("Factorization timed out! Potential DoS vulnerability."),
    }
}
