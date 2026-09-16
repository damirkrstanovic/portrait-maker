use std::fs;

use portrait_core::discovery::{
    DiscoveryEnvironment, DiscoveryPlatform, discover_destinations, discover_report,
    resolve_prefix, validate_destination,
};
use portrait_core::types::{DestinationOrigin, DestinationState, Game};

#[test]
fn steam_ids_and_windows_suffixes_are_game_specific() {
    assert_eq!(Game::Kingmaker.steam_id(), "640820");
    assert_eq!(Game::Wotr.steam_id(), "1184370");
    assert_eq!(
        Game::Kingmaker.windows_suffix(),
        "Owlcat Games/Pathfinder Kingmaker"
    );
    assert_eq!(
        Game::Wotr.windows_suffix(),
        "Owlcat Games/Pathfinder Wrath Of The Righteous"
    );
}

#[test]
fn discovers_native_linux_kingmaker_and_proton_candidates_without_native_wotr() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let steam = home.join(".local/share/Steam");
    let library = temp.path().join("Steam Library");
    fs::create_dir_all(steam.join("steamapps")).unwrap();
    fs::create_dir_all(library.join("steamapps/compatdata/640820/pfx/drive_c/users/steamuser/AppData/LocalLow/Owlcat Games/Pathfinder Kingmaker/Portraits")).unwrap();
    fs::create_dir_all(library.join("steamapps/compatdata/1184370/pfx/drive_c/users/steamuser/AppData/LocalLow/Owlcat Games/Pathfinder Wrath Of The Righteous/Portraits")).unwrap();
    fs::create_dir_all(home.join(".config/unity3d/Owlcat Games/Pathfinder Kingmaker/Portraits"))
        .unwrap();
    fs::write(
        steam.join("steamapps/libraryfolders.vdf"),
        format!(
            r#"
"libraryfolders"
{{
  "0" {{ "path" "{}" "apps" {{ "640820" "1" "1184370" "1" }} }}
}}
"#,
            library.display()
        ),
    )
    .unwrap();
    fs::write(
        library.join("steamapps/appmanifest_640820.acf"),
        "\"AppState\" { \"appid\" \"640820\" }",
    )
    .unwrap();
    fs::write(
        library.join("steamapps/appmanifest_1184370.acf"),
        "\"AppState\" { \"appid\" \"1184370\" }",
    )
    .unwrap();

    let destinations = discover_destinations(&DiscoveryEnvironment {
        platform: DiscoveryPlatform::Linux,
        home: home.clone(),
        xdg_config_home: Some(home.join(".config")),
        xdg_data_home: Some(home.join(".local/share")),
        steam_roots: vec![steam],
        windows_local_low: None,
    })
    .unwrap();

    assert!(destinations.iter().any(|d| d.game == Game::Kingmaker
        && d.origin == DestinationOrigin::Steam
        && d.state == DestinationState::Existing
        && d.path.ends_with("Pathfinder Kingmaker/Portraits")));
    assert!(destinations.iter().any(|d| {
        d.game == Game::Wotr
            && d.origin == DestinationOrigin::Steam
            && d.path
                .ends_with("Pathfinder Wrath Of The Righteous/Portraits")
    }));
    assert!(!destinations.iter().any(
        |d| d.game == Game::Wotr && d.evidence.iter().any(|item| item.contains("native Linux"))
    ));
}

#[test]
fn manual_prefix_ignores_public_and_keeps_uninitialized_state_visible() {
    let temp = tempfile::tempdir().unwrap();
    let prefix = temp.path().join("manual-prefix");
    fs::create_dir_all(prefix.join("drive_c/users/heroic-user/AppData/LocalLow/Owlcat Games/Pathfinder Wrath Of The Righteous/Portraits")).unwrap();
    fs::create_dir_all(prefix.join("drive_c/users/Public/AppData/LocalLow/Owlcat Games/Pathfinder Wrath Of The Righteous/Portraits")).unwrap();
    fs::create_dir_all(prefix.join("drive_c/users/new-user")).unwrap();

    let destinations = resolve_prefix(&prefix, Game::Wotr).unwrap();
    assert_eq!(destinations.len(), 2);
    assert!(destinations.iter().any(|d| d.path.ends_with(
        "heroic-user/AppData/LocalLow/Owlcat Games/Pathfinder Wrath Of The Righteous/Portraits"
    ) && d.state == DestinationState::Existing));
    assert!(
        destinations
            .iter()
            .any(|d| d.state == DestinationState::Uninitialized)
    );
    assert!(
        !destinations
            .iter()
            .any(|d| d.path.to_string_lossy().contains("Public"))
    );
}

#[test]
fn existing_proton_prefix_survives_a_missing_manifest_and_merges_steam_root_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let steam = home.join(".local/share/Steam");
    let portraits = steam.join("steamapps/compatdata/640820/pfx/drive_c/users/steamuser/AppData/LocalLow/Owlcat Games/Pathfinder Kingmaker/Portraits");
    fs::create_dir_all(&portraits).unwrap();
    let result = discover_destinations(&DiscoveryEnvironment {
        platform: DiscoveryPlatform::Linux,
        home: home.clone(),
        xdg_config_home: None,
        xdg_data_home: None,
        steam_roots: vec![steam.clone(), steam],
        windows_local_low: None,
    })
    .unwrap();
    let destination = result.iter().find(|item| item.path == portraits).unwrap();
    assert_eq!(destination.state, DestinationState::Existing);
    assert!(
        destination
            .evidence
            .iter()
            .any(|item| item.contains("manifest is absent"))
    );
}

#[test]
fn legacy_libraryfolders_discovers_a_secondary_library_prefix() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let steam = home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam");
    let secondary = temp.path().join("Second Steam Library");
    let portraits = secondary.join("steamapps/compatdata/1184370/pfx/drive_c/users/custom-user/AppData/LocalLow/Owlcat Games/Pathfinder Wrath Of The Righteous/Portraits");
    fs::create_dir_all(steam.join("steamapps")).unwrap();
    fs::create_dir_all(&portraits).unwrap();
    fs::write(
        steam.join("steamapps/libraryfolders.vdf"),
        format!("\"libraryfolders\" {{ \"1\" \"{}\" }}", secondary.display()),
    )
    .unwrap();
    let results = discover_destinations(&DiscoveryEnvironment {
        platform: DiscoveryPlatform::Linux,
        home,
        xdg_config_home: None,
        xdg_data_home: None,
        steam_roots: vec![steam],
        windows_local_low: None,
    })
    .unwrap();
    assert!(
        results
            .iter()
            .any(|destination| destination.path == portraits)
    );
}

#[test]
fn malformed_library_vdf_becomes_a_warning_while_a_second_root_still_discovers() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let broken = temp.path().join("broken-steam");
    let usable = temp.path().join("usable-steam");
    fs::create_dir_all(broken.join("steamapps")).unwrap();
    fs::write(broken.join("steamapps/libraryfolders.vdf"), "not { valid").unwrap();
    let portraits = usable.join("steamapps/compatdata/640820/pfx/drive_c/users/user/AppData/LocalLow/Owlcat Games/Pathfinder Kingmaker/Portraits");
    fs::create_dir_all(&portraits).unwrap();
    let report = discover_report(&DiscoveryEnvironment {
        platform: DiscoveryPlatform::Linux,
        home,
        xdg_config_home: None,
        xdg_data_home: None,
        steam_roots: vec![broken, usable],
        windows_local_low: None,
    })
    .unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("malformed"))
    );
    assert!(
        report
            .destinations
            .iter()
            .any(|destination| destination.path == portraits)
    );
}

#[test]
fn manual_destination_validation_does_not_create_a_portraits_directory() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("manual-game");
    fs::create_dir_all(&root).unwrap();
    let destination = validate_destination(&root, Game::Kingmaker).unwrap();
    assert_eq!(destination.state, DestinationState::MissingPortraits);
    assert!(!root.join("Portraits").exists());
}

#[test]
fn macos_aliases_only_return_existing_kingmaker_paths() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let aliases = [
        home.join("Library/Application Support/Owlcat Games/Pathfinder Kingmaker/Portraits"),
        home.join("Library/Application Support/unity.Owlcat Games.Pathfinder Kingmaker/Portraits"),
        home.join(
            "Library/Application Support/unity3d/Owlcat Games/Pathfinder Kingmaker/Portraits",
        ),
    ];
    for alias in &aliases {
        fs::create_dir_all(alias).unwrap();
    }
    let destinations = discover_destinations(&DiscoveryEnvironment {
        platform: DiscoveryPlatform::Macos,
        home,
        xdg_config_home: None,
        xdg_data_home: None,
        steam_roots: vec![],
        windows_local_low: None,
    })
    .unwrap();
    assert_eq!(destinations.len(), 3);
    assert!(
        destinations
            .iter()
            .all(|destination| destination.game == Game::Kingmaker)
    );
    assert!(aliases.iter().all(|alias| {
        destinations
            .iter()
            .any(|destination| destination.path == *alias)
    }));
}

#[test]
fn equivalent_destination_from_two_library_lists_merges_both_evidence_values() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let root_a = temp.path().join("root-a");
    let root_b = temp.path().join("root-b");
    let shared = temp.path().join("shared");
    for root in [&root_a, &root_b] {
        fs::create_dir_all(root.join("steamapps")).unwrap();
        fs::write(
            root.join("steamapps/libraryfolders.vdf"),
            format!("\"libraryfolders\" {{ \"1\" \"{}\" }}", shared.display()),
        )
        .unwrap();
    }
    let portraits = shared.join("steamapps/compatdata/640820/pfx/drive_c/users/user/AppData/LocalLow/Owlcat Games/Pathfinder Kingmaker/Portraits");
    fs::create_dir_all(&portraits).unwrap();
    let result = discover_destinations(&DiscoveryEnvironment {
        platform: DiscoveryPlatform::Linux,
        home,
        xdg_config_home: None,
        xdg_data_home: None,
        steam_roots: vec![root_a.clone(), root_b.clone()],
        windows_local_low: None,
    })
    .unwrap();
    let destination = result.iter().find(|item| item.path == portraits).unwrap();
    assert!(
        destination
            .evidence
            .iter()
            .any(|e| e.contains(root_a.to_string_lossy().as_ref()))
    );
    assert!(
        destination
            .evidence
            .iter()
            .any(|e| e.contains(root_b.to_string_lossy().as_ref()))
    );
}

#[cfg(unix)]
#[test]
fn denied_native_root_warns_while_other_native_and_steam_candidates_continue() {
    use std::os::unix::fs::PermissionsExt;
    for platform in [
        DiscoveryPlatform::Linux,
        DiscoveryPlatform::Windows,
        DiscoveryPlatform::Macos,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let config = temp.path().join("config");
        let local_low = temp.path().join("LocalLow");
        let (blocked, usable, usable_game) = match platform {
            DiscoveryPlatform::Linux => (
                home.join(".config/unity3d/Owlcat Games/Pathfinder Kingmaker"),
                config.join("unity3d/Owlcat Games/Pathfinder Kingmaker/Portraits"),
                Game::Kingmaker,
            ),
            DiscoveryPlatform::Windows => (
                local_low.join("Owlcat Games/Pathfinder Kingmaker"),
                local_low.join("Owlcat Games/Pathfinder Wrath Of The Righteous/Portraits"),
                Game::Wotr,
            ),
            DiscoveryPlatform::Macos => (
                home.join("Library/Application Support/Owlcat Games/Pathfinder Kingmaker"),
                home.join(
                    "Library/Application Support/unity.Owlcat Games.Pathfinder Kingmaker/Portraits",
                ),
                Game::Kingmaker,
            ),
        };
        let steam = temp.path().join("steam");
        let proton = steam.join("steamapps/compatdata/640820/pfx/drive_c/users/u/AppData/LocalLow/Owlcat Games/Pathfinder Kingmaker/Portraits");
        fs::create_dir_all(&blocked).unwrap();
        fs::create_dir_all(&usable).unwrap();
        fs::create_dir_all(&proton).unwrap();
        fs::write(steam.join("steamapps/libraryfolders.vdf"), "\"unterminated").unwrap();
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).unwrap();
        let result = discover_report(&DiscoveryEnvironment {
            platform,
            home,
            xdg_config_home: Some(config),
            xdg_data_home: None,
            steam_roots: vec![steam.clone()],
            windows_local_low: Some(local_low),
        });
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700)).unwrap();
        let report = result.unwrap();
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| { warning.contains(blocked.to_string_lossy().as_ref()) }),
            "{platform:?}: missing native warning: {:?}",
            report.warnings
        );
        assert!(report.warnings.iter().any(|warning| {
            warning.contains(
                steam
                    .join("steamapps/libraryfolders.vdf")
                    .to_string_lossy()
                    .as_ref(),
            )
        }));
        assert!(report.destinations.iter().any(|item| {
            item.path == usable
                && item.game == usable_game
                && item.state == DestinationState::Existing
        }));
        assert!(report.destinations.iter().any(|item| item.path == proton));
        assert!(
            !report
                .destinations
                .iter()
                .any(|item| item.path == blocked.join("Portraits"))
        );
        assert!(!blocked.join("Portraits").exists());
    }
}

#[cfg(unix)]
#[test]
fn denied_prefix_is_a_discovery_warning_while_other_roots_continue() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let blocked = temp.path().join("blocked-steam");
    let usable = temp.path().join("usable-steam");
    let blocked_prefix = blocked.join("steamapps/compatdata/640820/pfx");
    let usable_portraits = usable.join("steamapps/compatdata/640820/pfx/drive_c/users/u/AppData/LocalLow/Owlcat Games/Pathfinder Kingmaker/Portraits");
    fs::create_dir_all(blocked_prefix.join("drive_c/users/u")).unwrap();
    fs::create_dir_all(&usable_portraits).unwrap();
    fs::set_permissions(&blocked_prefix, fs::Permissions::from_mode(0o000)).unwrap();
    let report = discover_report(&DiscoveryEnvironment {
        platform: DiscoveryPlatform::Linux,
        home: temp.path().join("home"),
        xdg_config_home: None,
        xdg_data_home: None,
        steam_roots: vec![blocked, usable],
        windows_local_low: None,
    })
    .unwrap();
    fs::set_permissions(&blocked_prefix, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("compatibility prefix"))
    );
    assert!(
        report
            .destinations
            .iter()
            .any(|item| item.path == usable_portraits)
    );
}

#[cfg(unix)]
#[test]
fn denied_prefix_user_returns_a_manual_filesystem_error() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let prefix = temp.path().join("prefix");
    let users = prefix.join("drive_c/users");
    fs::create_dir_all(users.join("blocked")).unwrap();
    fs::set_permissions(&users, fs::Permissions::from_mode(0o000)).unwrap();
    let error = resolve_prefix(&prefix, Game::Kingmaker).unwrap_err();
    fs::set_permissions(&users, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(error.code(), "FILESYSTEM_ERROR");
}

#[cfg(unix)]
#[test]
fn denied_user_returns_a_manual_filesystem_error() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let prefix = temp.path().join("prefix");
    let user = prefix.join("drive_c/users/blocked-user");
    fs::create_dir_all(&user).unwrap();
    fs::set_permissions(&user, fs::Permissions::from_mode(0o000)).unwrap();
    let error = resolve_prefix(&prefix, Game::Kingmaker).unwrap_err();
    fs::set_permissions(&user, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(error.code(), "FILESYSTEM_ERROR");
}

#[cfg(unix)]
#[test]
fn denied_prefix_game_root_returns_a_manual_filesystem_error() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("game-root");
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o000)).unwrap();
    let error = validate_destination(&root, Game::Kingmaker).unwrap_err();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(error.code(), "FILESYSTEM_ERROR");
}

#[test]
fn current_root_and_two_external_libraries_remain_distinct() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let root = temp.path().join("root");
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    fs::create_dir_all(root.join("steamapps")).unwrap();
    for (library, game) in [
        (&root, Game::Kingmaker),
        (&first, Game::Kingmaker),
        (&second, Game::Wotr),
    ] {
        fs::create_dir_all(library.join(format!(
            "steamapps/compatdata/{}/pfx/drive_c/users/u/AppData/LocalLow/{}/Portraits",
            game.steam_id(),
            game.windows_suffix()
        )))
        .unwrap();
    }
    fs::write(
        root.join("steamapps/libraryfolders.vdf"),
        format!(
            "\"libraryfolders\" {{ \"1\" {{ \"path\" \"{}\" }} \"2\" {{ \"path\" \"{}\" }} }}",
            first.display(),
            second.display()
        ),
    )
    .unwrap();
    let result = discover_destinations(&DiscoveryEnvironment {
        platform: DiscoveryPlatform::Linux,
        home,
        xdg_config_home: None,
        xdg_data_home: None,
        steam_roots: vec![root],
        windows_local_low: None,
    })
    .unwrap();
    assert_eq!(
        result
            .iter()
            .filter(|item| item.state == DestinationState::Existing)
            .count(),
        3
    );
}

#[test]
fn doubled_windows_backslashes_in_vdf_resolve_an_external_library() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("steam");
    let library = temp.path().join(r"Steam C:\\Library");
    let portraits = library.join("steamapps/compatdata/640820/pfx/drive_c/users/u/AppData/LocalLow/Owlcat Games/Pathfinder Kingmaker/Portraits");
    fs::create_dir_all(root.join("steamapps")).unwrap();
    fs::create_dir_all(&portraits).unwrap();
    let escaped = library.to_string_lossy().replace('\\', "\\\\");
    fs::write(
        root.join("steamapps/libraryfolders.vdf"),
        format!("\"libraryfolders\" {{ \"1\" {{ \"path\" \"{escaped}\" }} }}"),
    )
    .unwrap();
    let results = discover_destinations(&DiscoveryEnvironment {
        platform: DiscoveryPlatform::Linux,
        home: temp.path().join("home"),
        xdg_config_home: None,
        xdg_data_home: None,
        steam_roots: vec![root],
        windows_local_low: None,
    })
    .unwrap();
    assert!(results.iter().any(|item| item.path == portraits));
}

#[cfg(unix)]
#[test]
fn unreadable_vdf_becomes_a_warning_while_a_second_root_still_discovers() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let blocked = temp.path().join("blocked-steam");
    let usable = temp.path().join("usable-steam");
    let list = blocked.join("steamapps/libraryfolders.vdf");
    let portraits = usable.join("steamapps/compatdata/1184370/pfx/drive_c/users/u/AppData/LocalLow/Owlcat Games/Pathfinder Wrath Of The Righteous/Portraits");
    fs::create_dir_all(blocked.join("steamapps")).unwrap();
    fs::write(&list, "\"libraryfolders\" {}").unwrap();
    fs::create_dir_all(&portraits).unwrap();
    fs::set_permissions(&list, fs::Permissions::from_mode(0o000)).unwrap();
    let report = discover_report(&DiscoveryEnvironment {
        platform: DiscoveryPlatform::Linux,
        home: temp.path().join("home"),
        xdg_config_home: None,
        xdg_data_home: None,
        steam_roots: vec![blocked, usable],
        windows_local_low: None,
    })
    .unwrap();
    fs::set_permissions(&list, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("Cannot read Steam library list"))
    );
    assert!(
        report
            .destinations
            .iter()
            .any(|item| item.path == portraits)
    );
}

#[test]
#[ignore = "read-only local Steam discovery smoke; requires the developer's installed games"]
fn local_steam_discovery_keeps_native_and_proton_candidates_visible() {
    let destinations = discover_destinations(&DiscoveryEnvironment::production()).unwrap();
    let existing = destinations
        .iter()
        .filter(|destination| destination.state == DestinationState::Existing)
        .collect::<Vec<_>>();
    assert!(
        existing
            .iter()
            .filter(|destination| destination.game == Game::Kingmaker)
            .count()
            >= 2
    );
    assert!(
        existing
            .iter()
            .any(|destination| destination.game == Game::Wotr)
    );
}
