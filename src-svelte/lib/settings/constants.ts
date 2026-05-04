// settings/constants.ts — Frontend mirror of settings/defaults.rs.
// All hard-coded defaults, bounds, and labels for user-facing preferences.
// The Preferences dialog imports from here — no inline literals anywhere else.

// ── File & Path ───────────────────────────────────────────────────────────────

export const JPEG_QUALITY_DEFAULT = 75;
export const JPEG_QUALITY_MIN = 1;
export const JPEG_QUALITY_MAX = 100;

export const RECENT_DIRS_DEFAULT = 10;
export const RECENT_DIRS_MIN = 1;
export const RECENT_DIRS_MAX = 50;

// ── pcode / Macro ─────────────────────────────────────────────────────────────

export const CONSOLE_HISTORY_DEFAULT = 500;
export const CONSOLE_HISTORY_MIN = 100;
export const CONSOLE_HISTORY_MAX = 5000;

export const MACRO_FONT_DEFAULT = 13;
export const MACRO_FONT_MIN = 8;
export const MACRO_FONT_MAX = 24;

// ── Performance ───────────────────────────────────────────────────────────────

export const BUFFER_POOL_DEFAULT_GB = 4;
export const BUFFER_POOL_MIN_GB = 0.5;
export const BUFFER_POOL_MAX_GB = 32;

// Conversion helpers — DB stores bytes, UI shows GB
export const GB = 1024 * 1024 * 1024;

// ── AutoStretch ───────────────────────────────────────────────────────────────

export const SHADOW_CLIP_DEFAULT = -2.8;
export const SHADOW_CLIP_MIN = -5.0;
export const SHADOW_CLIP_MAX = 0.0;

export const TARGET_BG_DEFAULT = 0.15;
export const TARGET_BG_MIN = 0.01;
export const TARGET_BG_MAX = 0.50;

// ── Field metadata — used by Preferences dialog for labels and helper text ────

export interface PrefFieldMeta {
  key: string;
  label: string;
  helper: string;
  type: 'integer' | 'float' | 'path';
  min?: number;
  max?: number;
  default: number | string;
  unit?: string;  // displayed after the input
}

export const PREF_FIELDS: PrefFieldMeta[] = [
  // §5.2 File & Path
  {
    key: 'jpeg_quality',
    label: 'JPEG Quality',
    helper: 'Quality level for JPEG exports (1–100).',
    type: 'integer',
    min: JPEG_QUALITY_MIN,
    max: JPEG_QUALITY_MAX,
    default: JPEG_QUALITY_DEFAULT,
    unit: '%',
  },
  {
    key: 'recent_directories_max',
    label: 'Recent Directories',
    helper: 'Number of recent directories to remember (1–50).',
    type: 'integer',
    min: RECENT_DIRS_MIN,
    max: RECENT_DIRS_MAX,
    default: RECENT_DIRS_DEFAULT,
  },
  // §5.3 pcode / Macro
  {
    key: 'backup_directory',
    label: 'Backup Directory',
    helper: 'Destination folder for database backups.',
    type: 'path',
    default: '',
  },
  {
    key: 'console_history_size',
    label: 'Console History Size',
    helper: 'Maximum number of commands to retain in console history (100–5000).',
    type: 'integer',
    min: CONSOLE_HISTORY_MIN,
    max: CONSOLE_HISTORY_MAX,
    default: CONSOLE_HISTORY_DEFAULT,
  },
  {
    key: 'macro_editor_font_size',
    label: 'Macro Editor Font Size',
    helper: 'Font size in the macro editor (8–24).',
    type: 'integer',
    min: MACRO_FONT_MIN,
    max: MACRO_FONT_MAX,
    default: MACRO_FONT_DEFAULT,
    unit: 'px',
  },
  // §5.4 Performance
  {
    key: 'buffer_pool_memory_limit',
    label: 'Buffer Pool Memory Limit',
    helper: 'Maximum memory for image buffers (0.5–32 GB). Takes effect on next session.',
    type: 'float',
    min: BUFFER_POOL_MIN_GB,
    max: BUFFER_POOL_MAX_GB,
    default: BUFFER_POOL_DEFAULT_GB,
    unit: 'GB',
  },
  // §5.7 AutoStretch
  {
    key: 'autostretch_shadow_clip',
    label: 'AutoStretch Shadow Clip',
    helper: 'Shadow clipping parameter for Auto-STF (-5.0–0.0). Default is -2.8.',
    type: 'float',
    step: 0.1,
    min: SHADOW_CLIP_MIN,
    max: SHADOW_CLIP_MAX,
    default: SHADOW_CLIP_DEFAULT,
  },
  {
    key: 'autostretch_target_bg',
    label: 'AutoStretch Target Background',
    helper: 'Target background level for Auto-STF (0.01–0.50). Default is 0.15.',
    type: 'float',
    step: 0.05,
    min: TARGET_BG_MIN,
    max: TARGET_BG_MAX,
    default: TARGET_BG_DEFAULT,
  },
];

// ── Section grouping — used by Preferences dialog to render section headers ──

export interface PrefSection {
  title: string;
  keys: string[];
}

// ── Threshold Profiles ────────────────────────────────────────────────────────

export const BG_MEDIAN_SIGMA_DEFAULT  = 2.5;
export const SNR_SIGMA_DEFAULT        = -2.5;
export const STAR_COUNT_SIGMA_DEFAULT = -3.0;
export const FWHM_SIGMA_DEFAULT       = 2.5;
export const ECCENTRICITY_ABS_DEFAULT = 0.85;
export const ECCENTRICITY_ABS_MIN     = 0.10;
export const ECCENTRICITY_ABS_MAX     = 1.00;
export const SIGMA_MIN                = 0.5;
export const SIGMA_MAX                = 4.0;
export const SIGMA_STEP               = 0.5;

export interface ThresholdFieldMeta {
  key:       string;
  label:     string;
  direction: '+' | '-';   // '+' = reject if above, '-' = reject if below
  type:      'sigma' | 'absolute';
  min:       number;
  max:       number;
  default:   number;
  step:      number;
}

export const THRESHOLD_FIELDS: ThresholdFieldMeta[] = [
  {
    key:       'bg_median_reject_sigma',
    label:     'Background Median',
    direction: '+',
    type:      'sigma',
    min:       SIGMA_MIN,
    max:       SIGMA_MAX,
    default:   BG_MEDIAN_SIGMA_DEFAULT,
    step:      SIGMA_STEP,
  },
  {
    key:       'snr_reject_sigma',
    label:     'SNR Estimate',
    direction: '-',
    type:      'sigma',
    min:       -4.0,
    max:       -0.5,
    default:   SNR_SIGMA_DEFAULT,
    step:      SIGMA_STEP,
  },
  {
    key:       'star_count_reject_sigma',
    label:     'Star Count',
    direction: '-',
    type:      'sigma',
    min:       -4.0,
    max:       -0.5,
    default:   STAR_COUNT_SIGMA_DEFAULT,
    step:      SIGMA_STEP,
  },
  {
    key:       'fwhm_reject_sigma',
    label:     'FWHM',
    direction: '+',
    type:      'sigma',
    min:       SIGMA_MIN,
    max:       SIGMA_MAX,
    default:   FWHM_SIGMA_DEFAULT,
    step:      SIGMA_STEP,
  },
  {
    key:       'eccentricity_reject_abs',
    label:     'Eccentricity',
    direction: '+',
    type:      'absolute',
    min:       ECCENTRICITY_ABS_MIN,
    max:       ECCENTRICITY_ABS_MAX,
    default:   ECCENTRICITY_ABS_DEFAULT,
    step:      0.01,
  },
];

export const PREF_SECTIONS: PrefSection[] = [
  {
    title: 'File & Path',
    keys: ['jpeg_quality', 'recent_directories_max'],
  },
  {
    title: 'pcode / Macro',
    keys: ['backup_directory', 'console_history_size', 'macro_editor_font_size'],
  },
  {
    title: 'Performance',
    keys: ['buffer_pool_memory_limit'],
  },
  {
    title: 'AutoStretch',
    keys: ['autostretch_shadow_clip', 'autostretch_target_bg'],
  },
];
