//! Meetings: live call transcription (mic = "Me", system audio = "Them").
//! Phase 0: the segmenter and the S4 echo check; capture and storage land
//! in Phase 1.

pub mod echo;
pub mod segmenter;
