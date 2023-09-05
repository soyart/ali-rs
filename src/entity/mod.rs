pub mod blockdev;
pub mod report;

use humanize_rs::bytes;

use crate::errors::AliError;

pub fn parse_human_bytes(s: &str) -> std::result::Result<bytes::Bytes, AliError> {
    (s.to_lowercase())
        .parse::<bytes::Bytes>()
        .map_err(|err| AliError::BadManifest(format!("bad byte unit string {s}: {err}")))
}

#[test]
#[rustfmt::skip]
fn test_is_valid_size() {
    // These size must not exceed usize
    let valids = vec![
        "1ki", "1kib", "1Ki", "1Kib", "1KiB", "1KIB",
        "1mi", "1mib", "1Mi", "1Mib", "1MiB", "1MIB",
        "1gi", "1gib", "1Gi", "1Gib", "1GiB", "1GIB",
        "1ti", "1tib", "1Ti", "1Tib", "1TiB", "1TIB",
        "1pi", "1pib", "1Pi", "1Pib", "1PiB", "1PIB",
        "1ei", "1eib", "1Ei", "1Eib", "1EiB", "1EIB",
        "1k", "1kb", "1K", "1Kb", "1KB",
        "1m", "1mb", "1M", "1Mb", "1MB",
        "1g", "1gb", "1G", "1Gb", "1GB",
        "1t", "1tb", "1T", "1Tb", "1TB",
        "1p", "1pb", "1P", "1Pb", "1PB",
        "1e", "1eb", "1E", "1Eb", "1EB",

        "0ki", "0kib", "0Ki", "0Kib", "0KiB", "0KIB",
        "0mi", "0mib", "0Mi", "0Mib", "0MiB", "0MIB",
        "0gi", "0gib", "0Gi", "0Gib", "0GiB", "0GIB",
        "0ti", "0tib", "0Ti", "0Tib", "0TiB", "0TIB",
        "0pi", "0pib", "0Pi", "0Pib", "0PiB", "0PIB",
        "0ei", "0eib", "0Ei", "0Eib", "0EiB", "0EIB",
        "0k", "0kb", "0K", "0Kb", "0KB",
        "0m", "0mb", "0M", "0Mb", "0MB",
        "0g", "0gb", "0G", "0Gb", "0GB",
        "0t", "0tb", "0T", "0Tb", "0TB",
        "0p", "0pb", "0P", "0Pb", "0PB",
        "0e", "0eb", "0E", "0Eb", "0EB",

        "01 ki", "01 kib", "01 Ki", "01 Kib", "01 KiB", "01 KIB",
        "01 mi", "01 mib", "01 Mi", "01 Mib", "01 MiB", "01 MIB",
        "01 gi", "01 gib", "01 Gi", "01 Gib", "01 GiB", "01 GIB",
        "01 ti", "01 tib", "01 Ti", "01 Tib", "01 TiB", "01 TIB",
        "01 pi", "01 pib", "01 Pi", "01 Pib", "01 PiB", "01 PIB",
        "01 ei", "01 eib", "01 Ei", "01 Eib", "01 EiB", "01 EIB",
        "01 k", "01 kb", "01 K", "01 Kb", "01 KB",
        "01 m", "01 mb", "01 M", "01 Mb", "01 MB",
        "01 g", "01 gb", "01 G", "01 Gb", "01 GB",
        "01 t", "01 tb", "01 T", "01 Tb", "01 TB",
        "01 p", "01 pb", "01 P", "01 Pb", "01 PB",
        "01 e", "01 eb", "01 E", "01 Eb", "01 EB",

        "1 ki", "1 kib", "1 Ki", "1 Kib", "1 KiB", "1 KIB",
        "1 mi", "1 mib", "1 Mi", "1 Mib", "1 MiB", "1 MIB",
        "1 gi", "1 gib", "1 Gi", "1 Gib", "1 GiB", "1 GIB",
        "1 ti", "1 tib", "1 Ti", "1 Tib", "1 TiB", "1 TIB",
        "1 pi", "1 pib", "1 Pi", "1 Pib", "1 PiB", "1 PIB",
        "1 ei", "1 eib", "1 Ei", "1 Eib", "1 EiB", "1 EIB",
        "1 k", "1 kb", "1 K", "1 Kb", "1 KB",
        "1 m", "1 mb", "1 M", "1 Mb", "1 MB",
        "1 g", "1 gb", "1 G", "1 Gb", "1 GB",
        "1 t", "1 tb", "1 T", "1 Tb", "1 TB",
        "1 p", "1 pb", "1 P", "1 Pb", "1 PB",
        "1 e", "1 eb", "1 E", "1 Eb", "1 EB",

        "1  ki", "1  kib", "1  Ki", "1  Kib", "1  KiB", "1  KIB",
        "1  mi", "1  mib", "1  Mi", "1  Mib", "1  MiB", "1  MIB",
        "1  gi", "1  gib", "1  Gi", "1  Gib", "1  GiB", "1  GIB",
        "1  ti", "1  tib", "1  Ti", "1  Tib", "1  TiB", "1  TIB",
        "1  pi", "1  pib", "1  Pi", "1  Pib", "1  PiB", "1  PIB",
        "1  ei", "1  eib", "1  Ei", "1  Eib", "1  EiB", "1  EIB",
        "1  k", "1  kb", "1  K", "1  Kb", "1  KB",
        "1  m", "1  mb", "1  M", "1  Mb", "1  MB",
        "1  g", "1  gb", "1  G", "1  Gb", "1  GB",
        "1  t", "1  tb", "1  T", "1  Tb", "1  TB",
        "1  p", "1  pb", "1  P", "1  Pb", "1  PB",
        "1  e", "1  eb", "1  E", "1  Eb", "1  EB",

        "1    ki", "1    kib", "1    Ki", "1    Kib", "1    KiB", "1    KIB",
        "1    mi", "1    mib", "1    Mi", "1    Mib", "1    MiB", "1    MIB",
        "1    gi", "1    gib", "1    Gi", "1    Gib", "1    GiB", "1    GIB",
        "1    ti", "1    tib", "1    Ti", "1    Tib", "1    TiB", "1    TIB",
        "1    pi", "1    pib", "1    Pi", "1    Pib", "1    PiB", "1    PIB",
        "1    ei", "1    eib", "1    Ei", "1    Eib", "1    EiB", "1    EIB",
        "1    k", "1    kb", "1    K", "1    Kb", "1    KB",
        "1    m", "1    mb", "1    M", "1    Mb", "1    MB",
        "1    g", "1    gb", "1    G", "1    Gb", "1    GB",
        "1    t", "1    tb", "1    T", "1    Tb", "1    TB",
        "1    p", "1    pb", "1    P", "1    Pb", "1    PB",
        "1    e", "1    eb", "1    E", "1    Eb", "1    EB",

        "10 ki", "10 kib", "10 Ki", "10 Kib", "10 KiB", "10 KIB",
        "10 mi", "10 mib", "10 Mi", "10 Mib", "10 MiB", "10 MIB",
        "10 gi", "10 gib", "10 Gi", "10 Gib", "10 GiB", "10 GIB",
        "10 ti", "10 tib", "10 Ti", "10 Tib", "10 TiB", "10 TIB",
        "10 pi", "10 pib", "10 Pi", "10 Pib", "10 PiB", "10 PIB",
        "10 ei", "10 eib", "10 Ei", "10 Eib", "10 EiB", "10 EIB",
        "10 k", "10 kb", "10 K", "10 Kb", "10 KB",
        "10 m", "10 mb", "10 M", "10 Mb", "10 MB",
        "10 g", "10 gb", "10 G", "10 Gb", "10 GB",
        "10 t", "10 tb", "10 T", "10 Tb", "10 TB",
        "10 p", "10 pb", "10 P", "10 Pb", "10 PB",
        "10 e", "10 eb", "10 E", "10 Eb", "10 EB",
    ];

    for v in valids {
        if let Err(err) = parse_human_bytes(v) {
            panic!("{v} should be valid, but was invalid: {err}");
        };
    }

    let invalids = vec![
        // No sizes
        "ki", "kib", "Ki", "Kib", "KiB", "KIB",
        "mi", "mib", "Mi", "Mib", "MiB", "MIB",
        "gi", "gib", "Gi", "Gib", "GiB", "GIB",
        "ti", "tib", "Ti", "Tib", "TiB", "TIB",
        "pi", "pib", "Pi", "Pib", "PiB", "PIB",
        "ei", "eib", "Ei", "Eib", "EiB", "EIB",
        "k", "kb", "K", "Kb", "KB",
        "m", "mb", "M", "Mb", "MB",
        "g", "gb", "G", "Gb", "GB",
        "t", "tb", "T", "Tb", "TB",
        "p", "pb", "P", "Pb", "PB",
        "e", "eb", "E", "Eb", "EB",

        // Minus sizes
        "-1 ki", "-1 kib", "-1 Ki", "-1 Kib", "-1 KiB", "-1 KIB",
        "-1 mi", "-1 mib", "-1 Mi", "-1 Mib", "-1 MiB", "-1 MIB",
        "-1 gi", "-1 gib", "-1 Gi", "-1 Gib", "-1 GiB", "-1 GIB",
        "-1 ti", "-1 tib", "-1 Ti", "-1 Tib", "-1 TiB", "-1 TIB",
        "-1 pi", "-1 pib", "-1 Pi", "-1 Pib", "-1 PiB", "-1 PIB",
        "-1 ei", "-1 eib", "-1 Ei", "-1 Eib", "-1 EiB", "-1 EIB",
        "-1 k", "-1 kb", "-1 K", "-1 Kb", "-1 KB",
        "-1 m", "-1 mb", "-1 M", "-1 Mb", "-1 MB",
        "-1 g", "-1 gb", "-1 G", "-1 Gb", "-1 GB",
        "-1 t", "-1 tb", "-1 T", "-1 Tb", "-1 TB",
        "-1 p", "-1 pb", "-1 P", "-1 Pb", "-1 PB",
        "-1 e", "-1 eb", "-1 E", "-1 Eb", "-1 EB",

        // <1 decimal sizes
        "0.5 ki", "0.5 kib", "0.5 Ki", "0.5 Kib", "0.5 KiB", "0.5 KIB",
        "0.5 mi", "0.5 mib", "0.5 Mi", "0.5 Mib", "0.5 MiB", "0.5 MIB",
        "0.5 gi", "0.5 gib", "0.5 Gi", "0.5 Gib", "0.5 GiB", "0.5 GIB",
        "0.5 ti", "0.5 tib", "0.5 Ti", "0.5 Tib", "0.5 TiB", "0.5 TIB",
        "0.5 pi", "0.5 pib", "0.5 Pi", "0.5 Pib", "0.5 PiB", "0.5 PIB",
        "0.5 ei", "0.5 eib", "0.5 Ei", "0.5 Eib", "0.5 EiB", "0.5 EIB",
        "0.5 k", "0.6 kb", "0.6 K", "0.6 Kb", "0.6 KB",
        "0.5 m", "0.6 mb", "0.6 M", "0.6 Mb", "0.6 MB",
        "0.5 g", "0.6 gb", "0.6 G", "0.6 Gb", "0.6 GB",
        "0.5 t", "0.6 tb", "0.6 T", "0.6 Tb", "0.6 TB",
        "0.5 p", "0.6 pb", "0.6 P", "0.6 Pb", "0.6 PB",
        "0.5 e", "0.6 eb", "0.6 E", "0.6 Eb", "0.6 EB",

        // Decimal sizes
        "10.29 ki", "10.29 kib", "10.29 Ki", "10.29 Kib", "10.29 KiB", "10.29 KIB",
        "10.29 mi", "10.29 mib", "10.29 Mi", "10.29 Mib", "10.29 MiB", "10.29 MIB",
        "10.29 gi", "10.29 gib", "10.29 Gi", "10.29 Gib", "10.29 GiB", "10.29 GIB",
        "10.29 ti", "10.29 tib", "10.29 Ti", "10.29 Tib", "10.29 TiB", "10.29 TIB",
        "10.29 pi", "10.29 pib", "10.29 Pi", "10.29 Pib", "10.29 PiB", "10.29 PIB",
        "10.29 ei", "10.29 eib", "10.29 Ei", "10.29 Eib", "10.29 EiB", "10.29 EIB",
        "10.29 k", "10.29 kb", "10.29 K", "10.29 Kb", "10.29 KB",
        "10.29 m", "10.29 mb", "10.29 M", "10.29 Mb", "10.29 MB",
        "10.29 g", "10.29 gb", "10.29 G", "10.29 Gb", "10.29 GB",
        "10.29 t", "10.29 tb", "10.29 T", "10.29 Tb", "10.29 TB",
        "10.29 p", "10.29 pb", "10.29 P", "10.29 Pb", "10.29 PB",
        "10.29 e", "10.29 eb", "10.29 E", "10.29 Eb", "10.29 EB",

        // Bad units
        "kiib", "kbi", "mbi", "zb", "zib", "ab",
        "kibibyte", "kilobyte", "mibibyte", "megabyte", "gibibyte", "gigabyte",
        "kibibytes", "kilobytes", "mibibytes", "megabytes", "gibibytes", "gigabytes",

        // Too large
        "2000EiB", "500E", "200000000000TiB"
    ];

    for v in invalids {
        if let Ok(bytes) = parse_human_bytes(v) {
            panic!("{v} should be invalid, but got {bytes:?}");
        }
    }
}
