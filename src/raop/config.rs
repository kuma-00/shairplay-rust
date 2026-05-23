// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Fabian Schmieder

//! Single Source of Truth (SSOT) for global AirPlay receiver capabilities and profiles.

/// Global device hardware model advertised to Apple clients.
pub const GLOBAL_MODEL: &str = "AppleTV2,1";

/// RTSP/mDNS protocol version for AirPlay 2.
pub const AP2_PROTOVERS: &str = "1.1";

/// Software build/source version reported in GET /info and mDNS.
pub const AP2_SRCVERS: &str = "366.0";

/// AP2 statusFlags indicating standard receiver attributes (transient state).
pub const AP2_STATUS_FLAGS: u32 = 0x4;

// --- Screen Mirroring (Video) Display Specifications ---

/// Width in pixels advertised for the virtual display target.
pub const MIRRORING_WIDTH: i64 = 1920;

/// Height in pixels advertised for the virtual display target.
pub const MIRRORING_HEIGHT: i64 = 1080;

/// Frame rate advertised for the virtual display target.
pub const MIRRORING_FPS: i64 = 60;

/// Static UUID tag for the virtual display target.
pub const MIRRORING_UUID: &str = "shairplay_display";

/// Display features bitmask advertised for screen mirroring.
pub const MIRRORING_FEATURES: i64 = 2;
