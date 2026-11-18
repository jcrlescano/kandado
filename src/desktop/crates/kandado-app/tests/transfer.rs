//! Import and export through the application service, offline: reading a file
//! into a preview, importing with progress and cancelling, the first-run
//! offer, and exporting only after the account password is checked.

use std::{path::PathBuf, sync::Arc};

use kandado_app::{
    ExportFormat, Filter, ImportError, KandadoApp, KdfParams, Paths, Source,
    clipboard::{Clipboard, MemoryClipboard},
    device_key::MemoryDeviceKey,
    secrets::MemorySecrets,
    testing::seed_account,
    transfer::{ImportChoice, ImportProgress, TransferError},
};
use zeroize::Zeroizing;

const PASSWORD: &str = "tubig at langis sa kusina";
const PROTON: &str = include_str!("../../kandado-vault/tests/fixtures/proton-data.json");

struct Computer {
    dir: PathBuf,
    app: Arc<KandadoApp>,
}

impl Drop for Computer {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn computer(name: &str) -> Computer {
    let dir = std::env::temp_dir().join(format!("kandado-transfer-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let paths = Paths {
        data_dir: dir.clone(),
    };
    seed_account(&paths, "juan@example.test", PASSWORD);
    let app = KandadoApp::with_clipboard(
        paths,
        Arc::new(MemorySecrets::default()),
        Arc::new(MemoryDeviceKey::new()),
        Clipboard::new(Arc::new(MemoryClipboard::default())),
        KdfParams::MIN,
        Some("http://127.0.0.1:9"),
    )
    .unwrap();
    app.restore().await.unwrap();
    app.unlock_with_password(PASSWORD.into()).await.unwrap();
    Computer { dir, app }
}

async fn read(app: &KandadoApp) -> kandado_app::transfer::Reading {
    app.read_import_bytes(
        Source::ProtonPass,
        Zeroizing::new(PROTON.as_bytes().to_vec()),
        "proton-pass-export.json".into(),
        None,
    )
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_is_read_into_a_preview_with_no_secret_in_it() {
    let computer = computer("preview").await;
    let reading = read(&computer.app).await;
    let preview = &reading.preview;
    assert_eq!(preview.file_name, "proton-pass-export.json");
    assert_eq!(preview.size, PROTON.len() as u64);
    assert_eq!(
        (
            preview.found,
            preview.importable,
            preview.trashed,
            preview.aliases
        ),
        (10, 9, 1, 1)
    );
    assert_eq!(preview.vaults, ["Personal", "Work"]);
    assert_eq!(preview.skipped.len(), 1);
    let shown = format!("{preview:?}");
    for secret in ["gh-pass-1", "4111111111111111", "bahay kubo", "jira-pass"] {
        assert!(!shown.contains(secret), "{secret} is in the preview");
    }

    // The wrong source for the file, and an encrypted one.
    let wrong = computer
        .app
        .read_import_bytes(
            Source::OnePassword,
            Zeroizing::new(PROTON.as_bytes().to_vec()),
            "x.1pux".into(),
            None,
        )
        .await;
    assert!(matches!(
        wrong,
        Err(TransferError::Import(ImportError::NotThisSource(
            "1Password"
        )))
    ));
    let missing = computer
        .app
        .read_import(Source::Csv, &computer.dir.join("nothing-here.csv"), None)
        .await;
    assert!(matches!(missing, Err(TransferError::Io(_))));
}

#[tokio::test(flavor = "multi_thread")]
async fn importing_reports_progress_can_be_cancelled_and_is_all_or_nothing() {
    let computer = computer("run").await;
    let reading = read(&computer.app).await;

    // Cancelled before it starts doing anything visible: nothing is kept.
    let progress = ImportProgress::new();
    progress.cancel();
    let result = computer
        .app
        .run_import(
            reading.plan.clone(),
            ImportChoice::MatchSource,
            true,
            progress.clone(),
        )
        .await;
    assert!(matches!(result, Err(TransferError::Cancelled)));
    assert_eq!(computer.app.counts(), (0, 0));
    assert_eq!(computer.app.vaults().unwrap().len(), 1);
    assert!(computer.app.last_import().is_none());

    // Run to the end, by the source's vaults.
    let progress = ImportProgress::new();
    let report = computer
        .app
        .run_import(
            reading.plan.clone(),
            ImportChoice::MatchSource,
            true,
            progress.clone(),
        )
        .await
        .unwrap();
    assert_eq!(
        (report.imported, report.in_trash, report.vaults_created),
        (9, 1, 1)
    );
    assert_eq!(progress.get(), (9, 9));
    assert_eq!(computer.app.counts(), (8, 1));
    assert_eq!(computer.app.last_import().unwrap(), report);
    let names: Vec<String> = computer
        .app
        .vaults()
        .unwrap()
        .into_iter()
        .map(|vault| vault.meta.name)
        .collect();
    assert_eq!(names, ["Personal", "Work"]);
    // The items are searchable at once.
    assert_eq!(computer.app.search("jira").len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn one_chosen_vault_and_no_trash() {
    let computer = computer("into").await;
    let reading = read(&computer.app).await;
    let personal = computer.app.vaults().unwrap()[0].id;
    let report = computer
        .app
        .run_import(
            reading.plan,
            ImportChoice::Into(personal),
            false,
            ImportProgress::new(),
        )
        .await
        .unwrap();
    assert_eq!(
        (
            report.imported,
            report.trash_left_out,
            report.vaults_created
        ),
        (8, 1, 0)
    );
    assert_eq!(computer.app.counts(), (8, 0));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_first_run_offer_shows_until_answered_or_used() {
    let computer = computer("offer").await;
    assert!(computer.app.should_offer_import());
    computer.app.import_offer_done().unwrap();
    assert!(!computer.app.should_offer_import());

    // A vault that already has items is not offered it either.
    let other = self::computer("offer-2").await;
    let reading = read(&other.app).await;
    other
        .app
        .run_import(
            reading.plan,
            ImportChoice::MatchSource,
            true,
            ImportProgress::new(),
        )
        .await
        .unwrap();
    assert!(!other.app.should_offer_import());
}

#[tokio::test(flavor = "multi_thread")]
async fn export_needs_the_account_password_and_what_it_writes_imports_back() {
    let source = computer("export").await;
    let reading = read(&source.app).await;
    source
        .app
        .run_import(
            reading.plan,
            ImportChoice::MatchSource,
            true,
            ImportProgress::new(),
        )
        .await
        .unwrap();
    assert_eq!(source.app.export_counts(), (9, 6));

    let file = source.dir.join("backup.kandado");
    // A wrong password writes nothing.
    let wrong = source
        .app
        .export_to(&file, ExportFormat::Encrypted, "not it".into())
        .await;
    assert!(matches!(wrong, Err(TransferError::WrongPassword)));
    assert!(!file.exists());

    let summary = source
        .app
        .export_to(&file, ExportFormat::Encrypted, PASSWORD.into())
        .await
        .unwrap();
    assert_eq!((summary.items, summary.omitted), (9, 0));
    assert!(file.exists());

    // Into another computer, with the password the file was made with.
    let target = computer("export-target").await;
    assert!(matches!(
        target.app.read_import(Source::Kandado, &file, None).await,
        Err(TransferError::Import(ImportError::NeedsPassphrase))
    ));
    assert!(matches!(
        target
            .app
            .read_import(Source::Kandado, &file, Some("not it".into()))
            .await,
        Err(TransferError::Import(ImportError::WrongPassphrase))
    ));
    let reading = target
        .app
        .read_import(Source::Kandado, &file, Some(PASSWORD.into()))
        .await
        .unwrap();
    let report = target
        .app
        .run_import(
            reading.plan,
            ImportChoice::MatchSource,
            true,
            ImportProgress::new(),
        )
        .await
        .unwrap();
    assert_eq!(report.imported, 9);
    assert_eq!(target.app.counts(), source.app.counts());
    let titles = |app: &KandadoApp| -> Vec<String> {
        let mut titles: Vec<String> = app
            .entries(Filter::default())
            .into_iter()
            .map(|entry| entry.title)
            .collect();
        titles.sort();
        titles
    };
    assert_eq!(titles(&target.app), titles(&source.app));

    // CSV says what it left out.
    let csv = source.dir.join("backup.csv");
    let summary = source
        .app
        .export_to(&csv, ExportFormat::Csv, PASSWORD.into())
        .await
        .unwrap();
    assert_eq!((summary.items, summary.omitted), (6, 2));
}
