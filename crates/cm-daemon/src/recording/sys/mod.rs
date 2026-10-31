//! OS-specific parts of the recording sources (`target_os` belongs in a `sys/` folder, see
//! `scripts/check_target_os.py`). The Linux sources compile on every system so that their logic is
//! tested with scripted fakes on the development Mac; only the hardware behind
//! [`linux::RealLinuxEnv`] needs Linux.

pub mod linux;
