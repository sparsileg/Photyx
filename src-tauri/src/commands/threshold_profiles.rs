// commands/threshold_profiles.rs — Threshold profile Tauri command handlers

use std::sync::Arc;
use tauri::State;
use crate::PhotoxState;
use crate::db;
use crate::settings::{ThresholdProfile, defaults::*};

#[tauri::command]
pub fn get_threshold_profiles(state: State<Arc<PhotoxState>>) -> Result<Vec<ThresholdProfile>, String> {
    let settings = state.settings.lock().expect("settings lock poisoned");
    Ok(settings.threshold_profiles.clone())
}

#[tauri::command]
pub fn get_active_threshold_profile_id(state: State<Arc<PhotoxState>>) -> Option<i64> {
    let settings = state.settings.lock().expect("settings lock poisoned");
    settings.active_threshold_profile_id
}

#[tauri::command]
pub fn save_threshold_profile(
    profile: ThresholdProfile,
    state: State<Arc<PhotoxState>>,
) -> Result<ThresholdProfile, String> {
    let db = state.db.lock().expect("db lock poisoned");
    let mut settings = state.settings.lock().expect("settings lock poisoned");

    // Clamp all values before writing
    let p = ThresholdProfile {
        id:                         profile.id,
        name:                       profile.name.trim().to_string(),
        description:                profile.description,
        bg_median_reject_sigma:     profile.bg_median_reject_sigma
                                        .clamp(BG_MEDIAN_SIGMA_MIN, BG_MEDIAN_SIGMA_MAX),
        signal_weight_reject_sigma: profile.signal_weight_reject_sigma
                                        .clamp(-SIGNAL_WEIGHT_SIGMA_MAX, -SIGNAL_WEIGHT_SIGMA_MIN),
        fwhm_reject_sigma:          profile.fwhm_reject_sigma
                                        .clamp(FWHM_SIGMA_MIN, FWHM_SIGMA_MAX),
        star_count_reject_sigma:    profile.star_count_reject_sigma
                                        .clamp(-STAR_COUNT_SIGMA_MAX, -STAR_COUNT_SIGMA_MIN),
        eccentricity_reject_abs:    profile.eccentricity_reject_abs
                                        .clamp(ECCENTRICITY_ABS_MIN, ECCENTRICITY_ABS_MAX),
    };

    if p.name.is_empty() {
        return Err("Profile name cannot be empty".to_string());
    }

    let now = db::now_unix();

    if p.id == 0 {
        // New profile — insert
        db.execute(
            "INSERT INTO threshold_profiles
             (name, description,
              bg_median_reject_sigma,
              signal_weight_reject_sigma, fwhm_reject_sigma, star_count_reject_sigma,
              eccentricity_reject_abs, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            rusqlite::params![
                p.name, p.description,
                p.bg_median_reject_sigma,
                p.signal_weight_reject_sigma, p.fwhm_reject_sigma, p.star_count_reject_sigma,
                p.eccentricity_reject_abs, now
            ],
        ).map_err(|e| e.to_string())?;

        let new_id = db.last_insert_rowid();
        let saved = ThresholdProfile { id: new_id, ..p };
        settings.threshold_profiles.push(saved.clone());
        Ok(saved)
    } else {
        // Existing profile — update
        db.execute(
            "UPDATE threshold_profiles SET
                name                       = ?1,
                description                = ?2,
                bg_median_reject_sigma     = ?3,
                signal_weight_reject_sigma = ?4,
                fwhm_reject_sigma          = ?5,
                star_count_reject_sigma    = ?6,
                eccentricity_reject_abs    = ?7,
                updated_at                 = ?8
             WHERE id = ?9",
            rusqlite::params![
                p.name, p.description,
                p.bg_median_reject_sigma,
                p.signal_weight_reject_sigma, p.fwhm_reject_sigma, p.star_count_reject_sigma,
                p.eccentricity_reject_abs, now, p.id
            ],
        ).map_err(|e| e.to_string())?;

        // Update in-memory vec
        if let Some(existing) = settings.threshold_profiles.iter_mut().find(|x| x.id == p.id) {
            *existing = p.clone();
        }
        Ok(p)
    }
}

#[tauri::command]
pub fn delete_threshold_profile(
    id: i64,
    state: State<Arc<PhotoxState>>,
) -> Result<(), String> {
    let db = state.db.lock().expect("db lock poisoned");
    let mut settings = state.settings.lock().expect("settings lock poisoned");

    db.execute("DELETE FROM threshold_profiles WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| e.to_string())?;

    settings.threshold_profiles.retain(|p| p.id != id);

    // If active profile was deleted, clear the active id
    if settings.active_threshold_profile_id == Some(id) {
        settings.active_threshold_profile_id = None;
    }

    // If no profiles remain, seed the Default profile
    if settings.threshold_profiles.is_empty() {
        let now = db::now_unix();
        let p = ThresholdProfile::default_profile();
        db.execute(
            "INSERT INTO threshold_profiles
             (name, description,
              bg_median_reject_sigma,
              signal_weight_reject_sigma, fwhm_reject_sigma, star_count_reject_sigma,
              eccentricity_reject_abs, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            rusqlite::params![
                p.name, p.description,
                p.bg_median_reject_sigma,
                p.signal_weight_reject_sigma, p.fwhm_reject_sigma, p.star_count_reject_sigma,
                p.eccentricity_reject_abs, now
            ],
        ).map_err(|e| e.to_string())?;

        let new_id = db.last_insert_rowid();
        let seeded = ThresholdProfile { id: new_id, ..ThresholdProfile::default_profile() };
        settings.threshold_profiles.push(seeded);
        settings.active_threshold_profile_id = Some(new_id);

        // Persist the new active id
        let _ = db.execute(
            "INSERT INTO preferences (key, value, updated_at)
             VALUES ('active_threshold_profile_id', ?1, ?2)
             ON CONFLICT(key) DO UPDATE SET
                 value = excluded.value,
                 updated_at = excluded.updated_at",
            rusqlite::params![new_id, now],
        );
    }

    Ok(())
}

#[tauri::command]
pub fn set_active_threshold_profile(
    id: i64,
    state: State<Arc<PhotoxState>>,
) -> Result<(), String> {
    let db = state.db.lock().expect("db lock poisoned");
    let mut settings = state.settings.lock().expect("settings lock poisoned");

    // Verify the profile exists
    let profile = settings.threshold_profiles.iter().find(|p| p.id == id)
        .ok_or_else(|| format!("No threshold profile with id {}", id))?
        .clone();

    settings.save_preference("active_threshold_profile_id", &id.to_string(), &db)?;

    // Propagate new thresholds into AppContext immediately
    let mut ctx = state.context.lock().expect("context lock poisoned");
    ctx.analysis_thresholds = crate::analysis::session_stats::AnalysisThresholds {
        background_median: crate::analysis::session_stats::MetricThresholds { reject: profile.bg_median_reject_sigma as f32 },
        signal_weight:     crate::analysis::session_stats::MetricThresholds { reject: profile.signal_weight_reject_sigma.abs() as f32 },
        fwhm:              crate::analysis::session_stats::MetricThresholds { reject: profile.fwhm_reject_sigma as f32 },
        star_count:        crate::analysis::session_stats::MetricThresholds { reject: profile.star_count_reject_sigma.abs() as f32 },
        eccentricity:      crate::analysis::session_stats::MetricThresholds { reject: profile.eccentricity_reject_abs as f32 },
    };

    Ok(())
}
