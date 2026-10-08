//! Tests for what a finished group is registered as.

use std::path::PathBuf;

use gglib_core::download::Quantization;

use super::*;
use crate::manager::shard_group_tracker::ShardGroupTracker;
use crate::queue::ShardGroupId;

fn dir() -> PathBuf {
    PathBuf::from("models").join("owner_zeta-GGUF")
}

fn job(file: &str) -> CompletedJob {
    CompletedJob {
        primary_path: dir().join(file),
        all_paths: vec![dir().join(file)],
        repo_id: "owner/zeta-GGUF".to_string(),
        commit_sha: "rev:main".to_string(),
        quantization: Quantization::Q8_0,
        files: vec![file.to_string()],
    }
}

/// A group of `shards` weights files and, when asked, a projector whose name
/// sorts before every one of them.
fn group(shards: &[&str], with_projector: bool) -> Vec<ResolvedFile> {
    let weights = shards
        .iter()
        .map(|name| ResolvedFile::with_size(*name, 1_000));
    let projector = with_projector.then(|| ResolvedFile::projector("mmproj-F16.gguf", 300, None));
    weights.chain(projector).collect()
}

/// The group, complete, as the tracker hands it over.
fn complete(shards: &[&str], with_projector: bool) -> GroupComplete {
    let entries = group(shards, with_projector);
    let first = job(shards[0]);
    GroupComplete {
        ordered_paths: entries.iter().map(|file| dir().join(&file.path)).collect(),
        metadata: GroupMetadata::of(&first, entries),
    }
}

const THREE: [&str; 3] = [
    "zeta.Q8_0-00001-of-00003.gguf",
    "zeta.Q8_0-00002-of-00003.gguf",
    "zeta.Q8_0-00003-of-00003.gguf",
];

// ── The completed download ───────────────────────────────────────────────

#[test]
fn one_weights_file_and_a_projector_are_an_unsharded_model() {
    let download = complete(&["zeta.Q8_0.gguf"], true).into_completed_download(vec![]);

    assert_eq!(download.primary_path, dir().join("zeta.Q8_0.gguf"));
    assert_eq!(download.projector_path, Some(dir().join("mmproj-F16.gguf")));
    assert!(!download.is_sharded);
    assert_eq!(download.file_paths, None);
    assert_eq!(shard_count(&download), 1);
    assert_eq!(download.all_paths.len(), 2);
    assert_eq!(download.hf_file_entries.len(), 2, "a row for every file");
}

#[test]
fn three_shards_and_a_projector_are_three_shards() {
    let download = complete(&THREE, true).into_completed_download(vec![]);

    assert_eq!(download.primary_path, dir().join(THREE[0]));
    assert_eq!(download.projector_path, Some(dir().join("mmproj-F16.gguf")));
    assert!(download.is_sharded);
    let shards: Vec<_> = THREE.iter().map(|name| dir().join(name)).collect();
    assert_eq!(download.file_paths, Some(shards));
    assert_eq!(shard_count(&download), 3);
    assert_eq!(download.all_paths.len(), 4);
}

#[test]
fn a_group_without_a_projector_has_none() {
    let download = complete(&["zeta.Q8_0.gguf"], false).into_completed_download(vec![]);

    assert_eq!(download.projector_path, None);
    assert_eq!(download.primary_path, dir().join("zeta.Q8_0.gguf"));
    assert!(!download.is_sharded);
}

#[test]
fn the_tags_and_identity_are_carried_over() {
    let download =
        complete(&["zeta.Q8_0.gguf"], true).into_completed_download(vec!["vision".to_string()]);

    assert_eq!(download.hf_tags, ["vision"]);
    assert_eq!(download.repo_id, "owner/zeta-GGUF");
    assert_eq!(download.commit_sha, "rev:main");
    assert_eq!(download.quantization, Quantization::Q8_0);
}

// ── One identity for every file of a group ───────────────────────────────

/// The projector's own job names the projector as its file. The metadata it
/// computes is still the weights', so the tracker sees one group.
#[test]
fn the_projectors_job_computes_the_metadata_of_the_weights() {
    let entries = group(&["zeta.Q8_0.gguf"], true);

    let from_weights = GroupMetadata::of(&job("zeta.Q8_0.gguf"), entries.clone());
    let from_projector = GroupMetadata::of(&job("mmproj-F16.gguf"), entries);

    assert_eq!(from_weights, from_projector);
    assert_eq!(from_projector.primary_filename, "zeta.Q8_0.gguf");
}

#[test]
fn the_primary_filename_of_a_sharded_group_has_no_shard_number() {
    let metadata = GroupMetadata::of(&job(THREE[1]), group(&THREE, true));

    assert_eq!(metadata.primary_filename, "zeta.Q8_0.gguf");
}

#[test]
fn a_group_is_complete_at_every_file_the_projector_included() {
    let shard = ShardInfo::new(0, 3, THREE[0]);

    let with = GroupMetadata::of(&job(THREE[0]), group(&THREE, true));
    let without = GroupMetadata::of(&job(THREE[0]), group(&THREE, false));
    let unkept = GroupMetadata::of(&job(THREE[0]), vec![]);

    assert_eq!(with.expected_files(&shard), 4);
    assert_eq!(without.expected_files(&shard), 3);
    assert_eq!(unkept.expected_files(&shard), 3);
}

/// The whole path: each file's job reports to the tracker, the projector
/// last or first, and the group completes once with the weights primary.
#[test]
fn the_tracker_completes_a_group_whose_projector_finished_first() {
    let entries = group(&["zeta.Q8_0.gguf"], true);
    let group_id = ShardGroupId::new("owner/zeta-GGUF:Q8_0:0");
    let mut tracker = ShardGroupTracker::new();
    let weights_place = ShardInfo::new(0, 1, "zeta.Q8_0.gguf");

    let projector_job = job("mmproj-F16.gguf");
    let metadata = GroupMetadata::of(&projector_job, entries.clone());
    let after_projector = tracker.on_shard_done(
        &group_id,
        1,
        projector_job.primary_path,
        metadata.expected_files(&weights_place),
        &metadata,
    );
    assert!(after_projector.is_none(), "the weights are still to come");

    let weights_job = job("zeta.Q8_0.gguf");
    let metadata = GroupMetadata::of(&weights_job, entries);
    let done = tracker
        .on_shard_done(
            &group_id,
            0,
            weights_job.primary_path,
            metadata.expected_files(&weights_place),
            &metadata,
        )
        .expect("both files are on disk");

    let download = done.into_completed_download(vec![]);
    assert_eq!(download.primary_path, dir().join("zeta.Q8_0.gguf"));
    assert_eq!(download.projector_path, Some(dir().join("mmproj-F16.gguf")));
}

// ── The completion message ───────────────────────────────────────────────

#[test]
fn the_message_counts_the_weights_shards_and_names_the_projector() {
    let download = complete(&THREE, true).into_completed_download(vec![]);

    let message = completion_message(&download, None, None);

    assert!(message.starts_with("Downloaded 3 shards to "), "{message}");
    assert!(
        message.ends_with(", with its projector mmproj-F16.gguf"),
        "{message}"
    );
}

#[test]
fn the_message_of_one_weights_file_and_a_projector_says_model() {
    let download = complete(&["zeta.Q8_0.gguf"], true).into_completed_download(vec![]);

    let message = completion_message(&download, None, None);

    assert!(message.starts_with("Downloaded model to "), "{message}");
    assert!(!message.contains("shards"), "{message}");
}

#[test]
fn the_message_reports_a_projector_that_was_not_linked() {
    let download = complete(&["zeta.Q8_0.gguf"], true).into_completed_download(vec![]);

    let message = completion_message(&download, None, Some("it holds weights"));

    assert!(
        message.ends_with(". Its projector mmproj-F16.gguf was not linked: it holds weights"),
        "{message}"
    );
}

#[test]
fn the_message_without_a_projector_is_the_download_alone() {
    let download = complete(&["zeta.Q8_0.gguf"], false).into_completed_download(vec![]);

    assert_eq!(
        completion_message(&download, None, None),
        format!(
            "Downloaded model to {}",
            dir().join("zeta.Q8_0.gguf").display()
        )
    );
}

#[test]
fn the_message_names_weights_the_reader_refused_and_gives_its_reason() {
    let download = complete(&["zeta.Q8_0.gguf"], false).into_completed_download(vec![]);

    assert_eq!(
        completion_message(&download, Some("Invalid GGUF format: no magic"), None),
        format!(
            "Downloaded model to {}. zeta.Q8_0.gguf was added without its details: \
             Invalid GGUF format: no magic",
            dir().join("zeta.Q8_0.gguf").display()
        )
    );
}

/// The refused weights are the last thing said, after the projector, linked
/// or not. Of a sharded model they are the first shard, the file that was
/// read.
#[test]
fn the_message_reports_refused_weights_after_what_it_says_of_the_projector() {
    let download = complete(&THREE, true).into_completed_download(vec![]);
    let unread = format!(". {} was added without its details: no magic", THREE[0]);

    let linked = completion_message(&download, Some("no magic"), None);
    let unlinked = completion_message(&download, Some("no magic"), Some("it holds weights"));

    assert!(
        linked.ends_with(&format!(", with its projector mmproj-F16.gguf{unread}")),
        "{linked}"
    );
    assert!(
        unlinked.ends_with(&format!(
            ". Its projector mmproj-F16.gguf was not linked: it holds weights{unread}"
        )),
        "{unlinked}"
    );
}
