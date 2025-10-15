# Licensing and Attribution Notes

This project pulls together several upstream resources, each with its own licensing regime. The intent of this document is to summarize the obligations so the repository stays compliant.

## 1. Project Source Code

The Rust source code in this repository is released under the [Unlicense](../LICENSE), placing the work in the public domain. You may copy, modify, and redistribute it without restriction. If you prefer to consume it under explicit terms, treat it as licensed under the permissive Unlicense fallback language.

## 2. NASA/JPL NAIF SPICE Toolkit

We rely on the CSPICE toolkit supplied by NASA’s Navigation and Ancillary Information Facility (NAIF). NAIF publishes a set of “Rules Regarding Use of SPICE,” which function as the software license. Key excerpts and obligations:

- **Obtain From NAIF:** Users should download the toolkit directly from the NAIF site or an authorized NASA flight-project distribution. Mirror redistribution of the unmodified toolkit is prohibited without written clearance. Bundling the toolkit in a larger application is permitted (NAIF Rules §“Obtaining the SPICE Toolkit,” §“Toolkit Redistribution”).  
- **Redistribution:** You may include the toolkit as part of your software so long as it remains unmodified, or you clearly document any modifications you make. Any modified kernels must change metadata and filenames to reflect their new provenance (§“Toolkit Redistribution,” §“Modifications to SPICE Kernels”).  
- **Export:** SPICE has been designated “Technology and Software Publicly Available” (TSPA); export from NAIF is unrestricted. If you redistribute a derivative package yourself, you must ensure your distribution also qualifies or obtain your own export designation (§“Export”).  
- **Support & Modifications:** NAIF discourages modifying toolkit source. If you do, NAIF will not provide support, and you must mark the changes conspicuously (§“Modifications to SPICE Code”).  
- **Disclaimer / No Warranty:**  
  > “THE SOFTWARE IS PROVIDED ‘AS-IS’ … WITHOUT WARRANTY OF ANY KIND … IN NO EVENT SHALL CALTECH, JPL, OR NASA BE LIABLE FOR ANY DAMAGES … RECIPIENT BEARS ALL RISK … AND AGREES TO INDEMNIFY CALTECH AND NASA …” (§capitalized disclaimer near the end of the Rules page).  
  Keep this disclaimer in your documentation if you redistribute binaries that include SPICE.
- **Acknowledgement:** NAIF requests users acknowledge SPICE/NAIF and the teams that produced the data when publishing results (§“Acknowledgement”).  
- **Commercial Use:** Allowed; no fees or license payments are required (§“Commercial Use of SPICE”).  

**How this project complies:**  
The `cspice-sys` build script downloads the official CSPICE package at build time directly from `https://naif.jpl.nasa.gov/`. We do not vend a mirror copy, so the redistribution clause is satisfied. If you plan to publish prebuilt binaries that already contain CSPICE libraries, include the disclaimer above and link to the NAIF rules.

## 3. SPICE Kernels

- Kernels obtained from NAIF may be used by anyone. Redistribution of *unmodified* kernels is permitted; modified kernels must update metadata and filenames to identify the modifying party (§“Kernels Distribution,” §“Kernels Redistribution,” §“Modifications to SPICE Kernels”).  
- Some kernels distributed by other organizations may carry additional rules—respect any notices embedded in the kernel comments.

Our downloader fetches public kernels (`de440s.bsp`, `naif0012.tls`, `pck00011.tpc`) without modification. If you create or edit kernels, add comment blocks documenting authorship, purpose, and validation.

## 4. `cspice-sys` Rust Bindings (LGPL-3.0)

The `vendor/cspice-sys` crate is a lightly modified copy of Jacob Halsey’s [`cspice-sys`](https://github.com/jacob-pro/cspice-rs) crate, licensed under [LGPL-3.0](vendor/cspice-sys/LICENSE). Our modifications are limited to:

- Replacing the build-script HTTP client with a rustls-based `reqwest` configuration.
- Vendoring the crate source so downstream builds can proceed without the OpenSSL dependency.

Obligations imposed by the LGPL-3.0:

- You must preserve the LGPL license text and acknowledge modifications. The `vendor/cspice-sys` directory keeps the original license, and changes are tracked in git history.  
- If you distribute binaries linked against this crate, you must make the modified source (and build scripts) available so others can relink or replace the library (§4 of LGPL-3.0). Keeping the `vendor/cspice-sys` tree in the repository satisfies this requirement.  
- You must allow reverse engineering for debugging modifications (§4). Do not add EULA terms that forbid it.

## 5. Action Checklist

- [x] Decide and document an overall project license (Unlicense).  
- [x] Keep NAIF copyright/credit and warranty disclaimer with any binary release.  
- [x] Provide a pointer to the NAIF SPICE Rules page in documentation (`docs/LICENSING.md`).  
- [ ] If you distribute prebuilt binaries, include this licensing document (or equivalent) in the release package.  
- [ ] If you modify or extend kernels or the CSPICE code, state that clearly in kernel comments or code headers.

## 6. Useful Links

- NAIF SPICE Rules: https://naif.jpl.nasa.gov/naif/rules.html  
- SPICE Credit Guidance: https://naif.jpl.nasa.gov/naif/credit.html  
- NASA PDS OSS policy (SPICE exemption): https://nasa-pds.github.io/collaborate/jpl-pds-oss-policy.html  
- Original `cspice-sys` crate: https://github.com/jacob-pro/cspice-rs
