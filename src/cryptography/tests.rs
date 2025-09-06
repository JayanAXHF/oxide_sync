use super::*;
use color_eyre::eyre::Result;
use pretty_assertions::assert_eq;
use std::fs::{self, File};
use std::io::Write;
use tempfile::tempdir;

#[test]
fn calc_rolling_hash() {
    let test_str = "abcdefghijklmnopqrstuvwxyz";
    let bytes = test_str.as_bytes();
    let signer = WeakSignature::new(2, bytes.into());
    let hash_1 = signer.sign(0);
    dbg!(hash_1.get_signature());
    let hash_2 = signer.compute_next_signature(hash_1);
    dbg!(hash_2.get_signature());
}

#[test]
fn find_item() {
    let mut sig = IndexTable::new();

    let test_str = "abcdefghijklmnopqrstuvwxyz";
    let bytes = test_str.as_bytes();
    let signer = WeakSignature::new(2, bytes.into());
    let hash_1 = signer.sign(0);
    sig.add(hash_1.clone(), "pippo".to_owned(), 0);

    assert_eq!(
        sig.find(hash_1.get_signature()).unwrap().1,
        "pippo".to_owned()
    );
}

#[test]
fn dump_part() {
    let mut delta = Delta::new();
    delta.add_block(vec![b'a', b'b', b'c']);

    assert_eq!(delta.dump(), "abc".to_owned());
}

#[test]
fn dump_index() {
    let mut delta = Delta::new();
    delta.add_index(0);

    assert_eq!(delta.dump(), "<b*0*>".to_owned());
}

#[test]
fn dump_part_index() {
    let mut delta = Delta::new();
    delta.add_block(vec![b'a', b'b', b'c']);
    delta.add_index(0);

    assert_eq!(delta.dump(), "abc<b*0*>".to_owned());
}

// Diff and patch tests
#[test]
fn test_apply_with_blocks_and_indices() {
    let base = b"abcdefghijklmnopqrstuvwxyz".to_vec();
    let block_size = 5;

    // Build a delta:
    // - Take block 1 ("fghij")
    // - Add literal "XYZ"
    // - Take block 4 ("uvwxy")
    let mut delta = Delta::new();
    delta.add_index(1);
    delta.add_block(b"XYZ".to_vec());
    delta.add_index(4);

    let result = delta.apply(&base, block_size).unwrap();
    let expected = b"fghijXYZuvwxy".to_vec();

    assert_eq!(result, expected);
}

#[test]
fn test_patch_file_creates_expected_output() {
    let dir = tempdir().unwrap();
    let old_path = dir.path().join("old.txt");
    let new_path = dir.path().join("new.txt");

    // Write a base file
    let base_content = b"The quick brown fox jumps over the lazy dog".to_vec();
    {
        let mut f = File::create(&old_path).unwrap();
        f.write_all(&base_content).unwrap();
    }

    let block_size = 10;
    let mut delta = Delta::new();
    // Copy first block ("The quick ")
    delta.add_index(0);
    // Insert "RED "
    delta.add_block(b"RED ".to_vec());
    // Copy second block ("brown fox ")
    delta.add_index(1);

    // Apply patch_file
    delta.patch_file(&old_path, &new_path, block_size).unwrap();

    // Read result
    let new_content = fs::read_to_string(&new_path).unwrap();
    let expected = "The quick RED brown fox ";
    assert_eq!(new_content, expected);
}

#[test]
fn test_invalid_index_returns_error() {
    let base = b"1234567890".to_vec();
    let mut delta = Delta::new();
    delta.add_index(99); // invalid block index

    let result = delta.apply(&base, 5);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_delta_patch_reconstructs_file() -> Result<()> {
    // Simulated "old file"
    let old_data = b"Hello, world!\nThis is the old file.\n".to_vec();

    // Target "new file"
    let new_data = b"Hello, world!\nThis is the NEW file!!!\n".to_vec();

    // Build a delta manually:
    // - Reuse first block ("Hello, world!\nThis is the ")
    // - Replace "old file.\n" with "NEW file!!!\n"
    let mut delta = Delta::new();
    delta.add_block(b"Hello, world!\nThis is the ".to_vec());
    delta.add_block(b"NEW file!!!\n".to_vec());

    assert!(delta.is_valid());

    // Apply patch
    let reconstructed = delta.apply(&old_data, 0)?; // 0 = arbitrary block size for now

    // Verify reconstruction
    assert_eq!(reconstructed, new_data);
    Ok(())
}

#[test]
fn test_diff_and_apply_roundtrip() {
    let base = b"The quick brown fox jumps over the lazy dog";
    let new = b"The quick brown cat jumps over the lazy dog with style";

    let block_size = 8;

    // Compute the delta between base and new
    let delta = Delta::diff(base, new, block_size);

    // Apply the delta to reconstruct new from base
    let reconstructed = delta.apply(base, block_size).expect("apply should succeed");

    assert_eq!(
        reconstructed, new,
        "Reconstructed file must match the new file"
    );
}

#[test]
fn test_diff_with_small_tail() {
    let base = b"abcdefg";
    let new = b"abcxyzg"; // change middle, tail smaller than block size
    let block_size = 4;

    let delta = Delta::diff(base, new, block_size);
    let reconstructed = delta.apply(base, block_size).expect("apply should succeed");

    assert_eq!(reconstructed, new, "Handles tail shorter than block size");
}

#[test]
fn test_diff_all_new_data() {
    let base = b"aaaaaa";
    let new = b"bbbbbb";
    let block_size = 3;

    let delta = Delta::diff(base, new, block_size);
    let reconstructed = delta.apply(base, block_size).expect("apply should succeed");

    assert_eq!(
        reconstructed, new,
        "No matches should still reconstruct correctly"
    );
}

#[test]
fn test_diff_base_smaller_than_block() {
    let base = b"hi";
    let new = b"hello";
    let block_size = 4;

    let delta = Delta::diff(base, new, block_size);
    let reconstructed = delta.apply(base, block_size).expect("apply should succeed");

    assert_eq!(
        reconstructed, new,
        "Should work when base is smaller than block size"
    );
}
