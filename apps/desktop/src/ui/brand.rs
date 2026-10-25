//! Brand marks of the services CaptureMann talks to (YouTube, TikTok, Facebook, Google Drive, OneDrive,
//! Dropbox). Single-colour glyphs on a 24 grid, tinted like any other icon: they follow the theme's
//! text colour and never take the service's brand colour (owner decision, Oct 24, 2026). The glyphs come
//! from the owner's brand set (Simple Icons paths, CC0, and two SVG Repo files, MIT, for OneDrive and
//! Dropbox: licences in `assets/icons/BRAND-LICENSE.txt`); the names and marks belong to their owners and
//! are used only to say which service a row is about.

/// One service's mark: the asset name for `components::icon`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Brand {
    pub icon: &'static str,
}

pub const YOUTUBE: Brand = Brand {
    icon: "brand-youtube",
};
pub const TIKTOK: Brand = Brand {
    icon: "brand-tiktok",
};
pub const FACEBOOK: Brand = Brand {
    icon: "brand-facebook",
};
pub const GOOGLE_DRIVE: Brand = Brand {
    icon: "brand-google-drive",
};
pub const ONEDRIVE: Brand = Brand {
    icon: "brand-onedrive",
};
pub const DROPBOX: Brand = Brand {
    icon: "brand-dropbox",
};

/// The mark of a service by its display name ("YouTube", "Google Drive", ...).
pub fn by_name(name: &str) -> Option<Brand> {
    match name {
        "YouTube" => Some(YOUTUBE),
        "TikTok" => Some(TIKTOK),
        "Facebook" => Some(FACEBOOK),
        "Google Drive" => Some(GOOGLE_DRIVE),
        "OneDrive" => Some(ONEDRIVE),
        "Dropbox" => Some(DROPBOX),
        _ => None,
    }
}

/// The mark of a publish platform id (`youtube`, `tiktok`, `facebook`).
pub fn platform(id: &str) -> Option<Brand> {
    match id {
        "youtube" => Some(YOUTUBE),
        "tiktok" => Some(TIKTOK),
        "facebook" => Some(FACEBOOK),
        _ => None,
    }
}

/// The mark of a storage provider (`Kind::Local` has none).
pub fn storage(kind: cm_storage::Kind) -> Option<Brand> {
    match kind {
        cm_storage::Kind::GoogleDrive => Some(GOOGLE_DRIVE),
        cm_storage::Kind::OneDrive => Some(ONEDRIVE),
        cm_storage::Kind::Dropbox => Some(DROPBOX),
        cm_storage::Kind::Local => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::paint::ASSETS;

    #[test]
    fn every_mark_is_a_registered_asset() {
        for b in [YOUTUBE, TIKTOK, FACEBOOK, GOOGLE_DRIVE, ONEDRIVE, DROPBOX] {
            let path = format!("icons/{}.svg", b.icon);
            let (_, svg) = ASSETS
                .iter()
                .find(|(p, _)| *p == path)
                .unwrap_or_else(|| panic!("{path} is not in ASSETS"));
            assert!(svg.contains("viewBox=") && svg.contains("<path"));
        }
    }

    #[test]
    fn platforms_and_providers_map_to_their_marks() {
        assert_eq!(platform("youtube"), Some(YOUTUBE));
        assert_eq!(platform("vimeo"), None);
        assert_eq!(storage(cm_storage::Kind::GoogleDrive), Some(GOOGLE_DRIVE));
        assert_eq!(storage(cm_storage::Kind::Dropbox), Some(DROPBOX));
        assert_eq!(storage(cm_storage::Kind::Local), None);
    }

    #[test]
    fn the_craftmann_endorsement_is_a_registered_asset() {
        let (_, svg) = ASSETS
            .iter()
            .find(|(p, _)| *p == "brand/endorsement-craftmann.svg")
            .expect("the endorsement line is in ASSETS");
        assert!(svg.contains("viewBox=\"0 0 130.41 24\"") && svg.contains("a CraftMann app"));
    }
}
