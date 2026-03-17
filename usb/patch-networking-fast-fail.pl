#!/usr/bin/env perl

use strict;
use warnings;

my $path = shift @ARGV or die "usage: $0 <networking-script>\n";

open my $in, '<', $path or die "open $path: $!\n";
local $/;
my $text = <$in>;
close $in;

my $wait_guard = <<'EOF';
        # WIFI_FASTBOOT_WAIT_GUARD
        if [ -s /config/wpa_supplicant-wlan0.conf ] && [ -r /sys/class/net/eth0/carrier ] && [ "$(cat /sys/class/net/eth0/carrier)" = "0" ]; then
                echo "eth0 has no carrier; skipping eth0 DHCP wait because Wi-Fi autostart is configured"
                return 0
        fi

EOF

my $revert_guard = <<'EOF';
        # WIFI_FASTBOOT_REVERT_GUARD
        if [ -s /config/wpa_supplicant-wlan0.conf ] && [ -r /sys/class/net/eth0/carrier ] && [ "$(cat /sys/class/net/eth0/carrier)" = "0" ]; then
                echo "eth0 has no carrier; skipping eth0 DHCP wait because Wi-Fi autostart is configured"
                return 0
        fi

EOF

if (index($text, 'WIFI_FASTBOOT_WAIT_GUARD') < 0) {
    $text =~ s/(wait_for_dhcp\(\) \{\n)/$1$wait_guard/
        or die "failed to locate wait_for_dhcp() in $path\n";
}

if (index($text, 'WIFI_FASTBOOT_REVERT_GUARD') < 0) {
    if ($text =~ /# WIFI_FASTBOOT_GUARD/) {
        $text =~ s/# WIFI_FASTBOOT_GUARD/# WIFI_FASTBOOT_REVERT_GUARD/
            or die "failed to rename legacy revert guard marker in $path\n";
    } else {
        $text =~ s/(revert_to_dhcp_if_broken\(\) \{\n)/$1$revert_guard/
            or die "failed to locate revert_to_dhcp_if_broken() in $path\n";
    }
}

open my $out, '>', $path or die "write $path: $!\n";
print {$out} $text;
close $out;

exit 0;