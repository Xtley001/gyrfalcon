//! `cargo run --bin cu-profile`
//!
//! Per `tests/fixtures/README.md`, `cu_table.csv` is this binary's output
//! artifact, produced by profiling real routes — not checked-in data. As
//! shipped, this binary has nothing real to profile: that needs `bundler`
//! assembling instructions from real `RoutedCandidate`s produced by a live
//! `strategy`+`health`+`router` pipeline, which doesn't exist yet (see
//! `crates/sim/src/cu_profile.rs`'s module doc). Running this now proves
//! the harness mechanism works (a self-test against a trivial known
//! instruction) and stops there, rather than writing a `cu_table.csv`
//! full of numbers that don't correspond to anything real.

use gyrfalcon_sim::profile_instructions;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::system_instruction;

fn main() {
    println!("cu-profile: no real liquidation routes exist yet to profile — see this");
    println!("binary's module doc. Running the harness self-test instead:\n");

    let payer = Keypair::new();
    let to = Pubkey::new_unique();
    let ix = system_instruction::transfer(&payer.pubkey(), &to, 1_000);

    match profile_instructions(&[ix], &payer, &[]) {
        Ok(profile) => {
            println!("Harness self-test PASSED.");
            println!(
                "  A bare SPL System transfer consumed {} CU.",
                profile.compute_units_consumed
            );
            println!(
                "  This confirms profile_instructions() measures real CU — it is not a stand-in \
                 for any real liquidation route's cost."
            );
        }
        Err(e) => {
            eprintln!("Harness self-test FAILED: {e}");
            eprintln!("Fix the harness before trusting it to profile real routes.");
            std::process::exit(1);
        }
    }

    println!(
        "\nNo tests/fixtures/cu_table.csv was written. Once bundler assembles real routes, wire \
         them into profile_instructions() here and write the results to that path."
    );
}
