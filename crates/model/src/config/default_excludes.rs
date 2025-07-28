// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

pub const DEFAULT_EXCLUDES: &[&str] = &[
    // binary data with known extension
    ".ico$",
    ".png$",
    ".clf$",
    ".tar$",
    ".tar.bzip2$",
    ".subunit$",
    ".sqlite$",
    ".db$",
    ".bin$",
    ".pcap.log.txt$",
    ".pkl$",
    ".jar$",
    ".pyc$",
    // code
    ".py$",
    ".sh$",
    // font
    ".eot$",
    ".otf$",
    ".woff$",
    ".woff2$",
    ".ttf$",
    // config
    ".yaml$",
    ".ini$",
    ".conf$",
    // not relevant
    "job-output.json$",
    "zuul-manifest.json$",
    ".html$",
    // binary data with known location
    "cacerts$",
    "local/creds$",
    "/authkey$",
    "mysql/tc.log.txt$",
    "log/.tmp$",
    // swifts
    "object.builder$",
    "account.builder$",
    "container.builder$",
    // openshift
    "crio/pods/crictl_inspectp_",
    "crio/containers/crictl_inspect_",
    // system config
    "/etc/",
    "/proc/",
    "/sys/",
    "/var/lib/selinux/",
    "/venv/",
    // hidden files
    "/\\.[a-zA-Z0-9]",
];
