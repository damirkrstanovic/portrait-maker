use portrait_core::types::{Destination, DestinationOrigin, DestinationState, Game};
use portrait_manager_lib::state::DesktopState;

#[test]
fn saved_destination_is_revalidated_before_the_desktop_returns_it() {
    let temp = tempfile::tempdir().unwrap();
    let portraits = temp.path().join("Pathfinder Kingmaker/Portraits");
    std::fs::create_dir_all(&portraits).unwrap();
    let state = DesktopState::new(temp.path().join("config"));
    let saved = Destination {
        id: uuid::Uuid::new_v4(),
        game: Game::Kingmaker,
        name: "My Kingmaker".into(),
        path: portraits.clone(),
        origin: DestinationOrigin::Manual,
        state: DestinationState::Existing,
        evidence: vec!["saved".into()],
    };
    state.save_destination(saved).unwrap();

    std::fs::remove_dir(&portraits).unwrap();
    let returned = state.saved_destinations().unwrap();
    assert_eq!(returned.len(), 1);
    assert_eq!(returned[0].name, "My Kingmaker");
    assert_eq!(returned[0].state, DestinationState::MissingPortraits);
}
