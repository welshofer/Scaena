//! Sampling (SPEC §5): interpolate *resolved geometry* between two snapshots at
//! time `t`. Numbers lerp; colors in Oklab; transforms decomposed; paths with
//! equal command structure point-wise; text by word diff; chart marks by key.
//! Springs and easings come from `scaena_core::timeline`.
