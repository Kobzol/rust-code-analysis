use flate2::read::GzDecoder;
use std::io::Cursor;
use std::path::Path;
use tar::Archive;

pub fn download_top_n_crates(dir: &Path, n: u64) -> anyhow::Result<()> {
    use crates_io_api::{CratesQuery, Sort, SyncClient};
    let client = SyncClient::new(
        "rust-crate-analysis (berykubik@gmail.com)",
        std::time::Duration::from_millis(1000),
    )?;

    if !dir.is_dir() {
        std::fs::create_dir_all(&dir)?;
    }

    let mut downloaded = 0;
    let mut page = 1;
    while downloaded < n {
        let remaining = n - downloaded;
        let mut query = CratesQuery::builder()
            .sort(Sort::Downloads)
            .page_size(100.min(remaining))
            .build();

        query.set_page(page);
        let response = client.crates(query)?;
        for c in response.crates {
            downloaded += 1;
            let version = c.max_stable_version.unwrap_or(c.max_version);
            if dir.join(format!("{}-{version}", c.name)).is_dir() {
                continue;
            }

            let url = format!(
                "https://static.crates.io/crates/{}/{}-{version}.crate",
                c.name, c.name
            );
            let response = ureq::get(url).config().build().call()?;
            assert_eq!(response.status(), 200);
            let body = response
                .into_body()
                .with_config()
                .limit(100 * 1024 * 1024)
                .read_to_vec()?;
            let tar = GzDecoder::new(Cursor::new(body));
            let mut archive = Archive::new(tar);
            archive.unpack(&dir)?;
        }
        page += 1;
        eprintln!("Downloaded {downloaded} crates out of {n}");
    }

    Ok(())
}

pub fn download_git_repo(dir: &Path, owner: &str, name: &str) -> anyhow::Result<()> {
    let target_dir = dir.join(format!("{owner}_{name}"));
    if target_dir.is_dir() {
        return Ok(());
    }
    eprintln!("Downloading {owner}/{name}");
    let url = format!("https://github.com/{owner}/{name}/archive/master.tar.gz");
    let response = ureq::get(url).config().build().call()?;
    assert_eq!(response.status(), 200);
    let body = response
        .into_body()
        .with_config()
        .limit(100 * 1024 * 1024)
        .read_to_vec()?;
    let tar = GzDecoder::new(Cursor::new(body));
    let mut archive = Archive::new(tar);
    archive.unpack(&target_dir)?;
    Ok(())
}
