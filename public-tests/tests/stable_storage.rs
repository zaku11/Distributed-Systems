use assignment_1_solution::build_stable_storage;
use ntest::timeout;
use tempfile::tempdir;

#[test]
#[timeout(300)]
fn storage_retrieves_inserted_key() {
    // given
    let root_storage_dir = tempdir().unwrap();
    let mut storage = build_stable_storage(root_storage_dir.path().to_path_buf());

    // when
    let before_insertion = storage.get("key");
    storage.put("key", vec![1 as u8, 2, 3].as_slice()).unwrap();

    // then
    assert_eq!(storage.get("key").unwrap(), vec![1 as u8, 2, 3]);
    assert_eq!(before_insertion, None);
}
