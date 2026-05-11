#!/usr/bin/env bash
# Armbian extension: enable PREEMPT_RT for low-latency JACK audio.
#
# Placed in userpatches/extensions/ by build-orange-pi-image.sh.
# Armbian calls custom_kernel_config after the base .config is in place;
# kernel_config_set_y is provided by the Armbian build framework.

function custom_kernel_config__openrig_rt() {
	if [[ -f .config ]]; then
		display_alert "OpenRig" "Enabling CONFIG_PREEMPT_RT for low-latency audio" "info"
		kernel_config_set_n CONFIG_PREEMPT_VOLUNTARY
		kernel_config_set_n CONFIG_PREEMPT_NONE
		kernel_config_set_y CONFIG_PREEMPT_RT
	fi
	# Hash must always be added (even without .config) for kernel cache keying.
	kernel_config_modifying_hashes+=("CONFIG_PREEMPT_RT=y")
}
