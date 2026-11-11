# Signing the Windows release: options, comparison, recommendation

Research for release 0.3.2, item 3 of `specs/plans/release-0.3.2-plan.md` (decision D-W4 of Oct 30, 2026, decision O4 of
Nov 5, 2026 "no signing in 0.x"). Written Nov 11, 2026 from the vendors' own pages and Microsoft Learn, each read on that
day; every claim carries its source and a confidence mark (**high**: quoted from the vendor's or Microsoft's page; **medium**:
on the page but indirect or inconsistent; **low**: not found on an authoritative page, stated as an assumption). Nothing here
was built, bought, registered or applied for. Prices and rules change: re-read the linked page before acting on it.

## Recommendation

**Apply to SignPath Foundation (option 1) and sign the installer, the uninstaller and the five CaptureMann executables in
`windows-package.yml` through its GitHub Actions connector.** It is free for open-source projects, the private key never
leaves SignPath's HSM, GitHub Actions can sign without a person at a keyboard (each release still needs one approval click
by a team member in SignPath, which matches how the maintainer publishes today), its terms contain no country rule, and it
is the route D-W4 already pointed at. What it does not do: it does not remove the warning at once. Microsoft's own page says a
newly signed file is still "flagged as unrecognized until reputation accumulates", that EV certificates "no longer bypass
SmartScreen", and that building reputation "can take several weeks and hundreds of clean installs". The publisher shown to
users will be **SignPath Foundation**, not the maintainer's name. **Fallback:** if the application is refused or takes longer
than the 0.3.2 window, (a) ship 0.3.2 unsigned with the release-text paragraph in section 7 (what 0.3.1 did), and (b) start
a Microsoft Store listing as a second channel (option 5: individual accounts are free since Sep 2025, the Store re-signs an
MSIX with Microsoft's certificate and Store installs never see the SmartScreen download warning; it does not help the GitHub
download and needs MSIX packaging work). **Not recommended:** Microsoft Artifact Signing (individuals must be in the United
States or Canada; organisations only in 12 listed countries or regions), an EV certificate (no SmartScreen advantage any
more, about USD 972 a year), and an OV or individual certificate on a hardware card (cheapest about EUR 69 at Certum, but
the card cannot be used by GitHub Actions, so the maintainer would sign by hand on release day). **What the maintainer must
do himself:** fill in the SignPath application (repository, licence, release page, team), accept its terms, publish a
"Code signing policy" page with the required sentence, install the SignPath GitHub App, create the API token and put it in
the repository secrets, approve each release signing request, and confirm his country (it matters for every option but
SignPath). Confidence of the pick: **medium** (eligibility rules are public and CaptureMann meets them on paper; acceptance
and its timing are SignPath's decision and not published).

## Comparison

| Option | Cost | Eligibility (individual, country, open source) | Who holds the key | CI fit with `windows-package.yml` | SmartScreen effect on the day of release | Time to the first signed release |
|---|---|---|---|---|---|---|
| 1. SignPath Foundation | Free ("our services are free of charge") | OSI licence, no proprietary component, actively maintained, already released; no country rule found; publisher is SignPath Foundation | SignPath's HSM; "The code signing certificate is issued to SignPath Foundation" | Good: `signpath/github-action-submit-signing-request@v3`, GitHub-hosted runners required for OSS, one secret (API token) | Warning stays until reputation builds (OV-class certificate); publisher name shown | Application review (duration not published) plus the workflow work; realistic: weeks |
| 2. Microsoft Artifact Signing (formerly Trusted Signing) | "Starts at $9.99/month" (Microsoft page); billed from account creation, not pro-rated; needs a paid Azure subscription | Individuals **only in the United States or Canada**; organisations in US, CA, EU, UK, AU, NZ, JP, KR, SG, CH, NO, IL | Microsoft's service (FIPS 140-3 Level 3); certificate never handed out; short-lived certificates, timestamp required | Good: `azure/artifact-signing-action@v2`, OIDC via `azure/login@v3`, Windows runners only | Same as 1: reputation builds over time | Identity validation 1 to 20 business days (organisation); not open to the maintainer unless he is in a listed place |
| 3. OV or individual (IV) certificate from a CA | DigiCert lists USD 696 a year (OV, mapping on the page unclear); Certum "Open Source Code Signing" set EUR 69 (card and reader included, validity not stated); SSL.com prices not shown | Individuals: Certum open-source set and SSL.com IV are for natural persons; CA validation of a person's identity; country rules per CA not read | Hardware card or token on the maintainer's desk (CA/B Forum rule since June 1, 2023), or the CA's cloud HSM at extra cost | Poor with a card (no unattended CI); the CA's cloud signing can run in CI with its own secrets | Same: reputation builds; publisher shown is the person's name | Days for validation plus shipping of the card |
| 4. EV certificate | DigiCert lists USD 972 a year; token USD 120 (DigiCert) | Organisation required (SSL.com sells an "EV SP" for sole proprietors) | Hardware token or cloud HSM | As 3 | **No advantage:** "EV certificates no longer bypass SmartScreen" (Microsoft, page dated May 4, 2026) | As 3, longer validation |
| 5. Microsoft Store, MSIX | Free: "no registration fees for either account type" (new flow since Sep 10, 2025, ID plus selfie, "nearly 200 markets") | Individual account with a personal Microsoft account; Store certification; policies 10.x | Microsoft re-signs the MSIX ("The Store replaces any existing signature ... with a Microsoft certificate") | Separate pipeline: build an MSIX (`makeappx`), upload to Partner Center; no signing step in CI | Store installs: "never subject to SmartScreen download warnings"; the GitHub download is unchanged | Account same day; MSIX packaging and certification: unknown, likely weeks |
| 6. winget only (what 0.2.8 to 0.3.1 did) | Free | Pull request to `microsoft/winget-pkgs`; unsigned installers accepted | Nobody (nothing is signed) | Already done by `release-files.sh` and the manifest templates | No change for a browser download; whether a winget install itself shows SmartScreen is not stated by Microsoft (section 6) | Two pull requests open since 0.2.8 (472327, 477195) |
| 7. Do nothing, document the warning | Free | – | – | – | Edge "isn't commonly downloaded", then "Windows protected your PC" with no publisher name | Now |

## 1. SignPath Foundation (free code signing for open source)

Sources: `https://signpath.org/` (landing page), `https://signpath.org/terms` (the page is marked as a draft), `https://signpath.org/about`,
`https://signpath.org/apply` (form not readable without a browser), `https://docs.signpath.io/trusted-build-systems/github`,
`https://docs.signpath.io/artifact-configuration/reference`, `https://docs.signpath.io/signing-code`,
`https://nsis.sourceforge.io/Signing_an_Uninstaller_externally`; all read Nov 11, 2026. The marketing page
`about.signpath.io/product/open-source` could not be fetched (server header error, twice).

- **What it is.** SignPath Foundation is "currently operated by SignPath GmbH, the company behind SignPath.io"; it gives open-source projects
  a free SignPath.io subscription and signs with a certificate issued to the Foundation: "The code signing certificate is issued to SignPath
  Foundation" and "SignPath Foundation is the publisher of the project's signed software". It vouches for origin, not for the maintainer's
  identity: SignPath verifies "that the binary was built from your open source repository", and the key "is generated and stored on
  SignPath's Hardware Security Module (HSM)". Projects named on the landing page: Stellarium, Flameshot, GitExtensions, Layer 1 Lite DB.
  Confidence **high**.
- **Eligibility (terms).** "must not contain malware or potentially unwanted programs"; an "OSI-approved Open Source license" without
  commercial dual-licensing, for all components; "may not contain any proprietary, non open-source component" (system libraries allowed);
  "must be actively maintained" and "must already be released in the form that should be signed"; the functionality must be described on
  the download page. No country, age or team-size rule appears in the terms. CaptureMann on paper: Apache-2.0, released since 0.1.0,
  GitHub release pages describe the product; the bundled FFmpeg (LGPL), llama-server (MIT), sherpa-onnx and the models are open source.
  Confidence **high** for the rules, **medium** for "CaptureMann qualifies" (SignPath decides; "no independent arbitration mechanism").
- **What the project must publish.** A page titled with the words "Code signing policy", linked from the project's home page, stating
  exactly "Free code signing provided by SignPath.io, certificate by SignPath Foundation", listing team roles (Authors, Reviewers,
  Approvers) and members; a privacy statement (link to a privacy policy, or state that the program "will not transfer any information to
  other networked systems" unless requested; third-party component policies where users are affected). For CaptureMann the honest
  statement is: no telemetry; network traffic only when the user connects Google Drive or YouTube, downloads a model, or turns on a
  remote AI key. Example of such a page from another accepted project: `https://github.com/rcourtman/Pulse/blob/main/docs/CODE_SIGNING_POLICY.md`
  (sections: signing service, build and release controls, project roles, user privacy and system changes). Confidence **high**.
- **Rules of use.** Sign only your own binaries ("Upstream software may be included unsigned in packages"); no hacking tools; disclose
  data transfers, show them at install and make them switchable; "must not modify system configuration without proper warnings" (the
  installer's PATH section is opt-in and described, the Open-with section is described; keep it that way); provide uninstallation (the
  NSIS uninstaller does); "All team members must use multi-factor authentication" for SignPath and the repository; "each signing request
  must be approved by a team member trusted by the entire team". Artifact rule: "Product name metadata must match the project name" and
  "Product version metadata must be identical across each build". **The Windows executables have no version resource today**
  (`packaging/windows/README.md`, "Not done": icon and version resource need a `build.rs`), so a `build.rs` with a version resource for
  the five `.exe` files is a prerequisite, not a nicety. Confidence **high** for the rules, **high** for the missing resource.
- **Termination.** SignPath may "pause or terminate the subscription without prior notice" and "revoke the certificate effective
  immediately or retroactively"; a malware report can trigger revocation; "SignPath GmbH does not accept any liability". A retroactive
  revocation would invalidate every past release's signature at once (shared certificate across projects). Confidence **high** for the
  text, **medium** for the shared-certificate consequence (the terms say the certificate is the Foundation's; whether one certificate
  serves all projects is not stated).
- **CI fit.** Action `signpath/github-action-submit-signing-request@v3`, inputs `api-token` (secret), `organization-id`, `project-slug`,
  `signing-policy-slug`, `github-artifact-id` (from `actions/upload-artifact` v4+: `${{ steps.<id>.outputs.artifact-id }}`),
  `wait-for-completion` (default true), `output-artifact-directory`; outputs `signing-request-id`, `signing-request-web-url`,
  `signed-artifact-download-url`. Origin checks: "A build was actually performed by a GitHub workflow, not by some other entity in
  possession of the API token", "Origin metadata is provided by GitHub, not the build script, and can therefore not be forged", "The
  artifact is stored as a GitHub workflow artifact before it is submitted for signing". **For open-source projects all jobs leading to
  the signing request must run on GitHub-hosted agents** (`windows-package.yml` runs on `windows-latest`: fine). The SignPath GitHub
  App must be installed on the repository. Signing policies: a test-signing policy (automatic) and a release-signing policy (manual
  approval: "Project manager is notified about pending approval ... Approves or denies based on test results and verified origin
  data"). Confidence **high**.
- **What gets signed and how.** `pe-file` covers "EXE, DLL, and other executable files" and is **not** a composite type, so an NSIS
  installer is signed as one opaque EXE: the programs inside must be signed **before** `makensis` packs them, and the uninstaller needs
  the two-pass NSIS recipe (`!uninstfinalize` exists since NSIS 3.08; build once with `-DEXPORT_UNINST` to write `uninstall.exe`, sign it
  externally, build again with `-DIMPORT_UNINST` so the installer embeds the signed file; real example: smartmontools' `installer.nsi`
  and `.github/workflows/signpath.yml`). A `zip-file` **is** composite, so one signing request can sign all executables inside a zip.
  Expect three signing requests per release: the zip of executables, `uninstall.exe`, `setup.exe` (or two, if SignPath accepts a
  directory artifact with the uninstaller and installer together; ask). FFmpeg and llama-server stay unsigned ("Upstream software may be
  included unsigned"); whether SignPath lets a project sign binaries it builds from upstream source is a question for them. Timestamping:
  SignPath's product page lists "timestamping" among Semantic Code Signing features and its verification checks a "Valid timestamp (or
  unexpired publisher certificate)"; the exact timestamp authority is not on the pages read. Confidence **high** for file types,
  **medium** for timestamping, **low** for the number of requests.
- **Approval time.** Not published anywhere read; a third-party tutorial says "a few days". Treat as unknown; ask in the application.
  Confidence **low**.
- **Failure modes.** Refusal (no appeal); slow approval; a signing request waits for a human (the action's `wait-for-completion` has a
  timeout, the run fails if nobody approves); revocation; the publisher name "SignPath Foundation" may puzzle users who expect
  "CaptureMann" (the release text must explain it); a later move to the maintainer's own certificate restarts reputation ("changing your
  signing certificate affects the publisher trust signal").

## 2. Microsoft Artifact Signing (formerly Trusted Signing)

Sources: `https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart` (ms.date May 21, 2026, updated Oct 8, 2026),
`https://learn.microsoft.com/en-us/azure/artifact-signing/faq` (updated Oct 27, 2026), `https://azure.microsoft.com/en-us/pricing/details/artifact-signing/`,
`https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation` (ms.date May 4, 2026),
`https://github.com/Azure/trusted-signing-action` (now Artifact Signing Action); read Nov 11, 2026.

- **Eligibility.** "Public Trust certificates are available to organizations in the United States, Canada, the European Union, the United
  Kingdom, Australia, New Zealand, Japan, South Korea, Singapore, Switzerland, Norway, and Israel. Individual developers must be located in
  the United States or Canada." For an individual, the Azure billing account must be of type Individual and its legal name and sold-to
  address must match the government ID. "Artifact Signing doesn't support free, trial, or sponsored Azure subscriptions." Unless the
  maintainer lives in the US or Canada or owns an organisation in a listed place, **this option is closed**. Confidence **high**.
- **Cost.** The pricing page rendered its figures as "$-" placeholders when read; Microsoft's SmartScreen page says "Starts at
  $9.99/month"; Basic includes 5,000 signatures a month and 1 certificate profile of each type, Premium 100,000 and 10. "The service starts
  billing as soon as you create a Artifact Signing account", not pro-rated. Confidence **medium** on the price (second-hand within
  Microsoft Learn), **high** on quotas and billing rule.
- **Identity validation.** Organisation: 1 to 20 business days, documents issued within the last 12 months, three upload attempts.
  Individual: Verified ID through a partner (AU10TIX walk-through), government ID plus proof of address; name and city, state and
  country appear on the certificate, street and email do not. "Identity validation requests can't be expedited." Validation expires and
  must be renewed or signing stops. Confidence **high**.
- **Key and certificates.** Held by Microsoft, "FIPS 140-3 Level 3"; "The Authenticode certificate that's used for signing with the
  profile is never given to you"; the ELAM note says the issued certificate "is valid for three days", and the action's README: "The
  files must be signed with timestamping enabled in order for the signatures to be valid for longer than 3 days" (timestamp server
  `http://timestamp.acs.microsoft.com`). "Artifact Signing doesn't issue Extended Validation (EV) certificates." Custom CN or O is not
  possible. Confidence **high**.
- **CI fit.** `azure/artifact-signing-action@v2` with `endpoint`, `signing-account-name`, `certificate-profile-name`, file selection
  (`files-folder` with `files-folder-filter: exe`), `file-digest: SHA256`, `timestamp-rfc3161`, `timestamp-digest`; authentication by
  OIDC through `azure/login@v3` (`client-id`, `tenant-id`, `subscription-id`; `permissions: id-token: write`) or a client secret; the
  signer needs the "Artifact Signing Certificate Profile Signer" role. Runners: `windows-2022` or `windows-2025` only. Signs "all file
  types that SignTool supports" (the NSIS installer, the uninstaller and the executables all qualify; the same before-packing order as in
  option 1 applies). Confidence **high**.
- **SmartScreen.** "SmartScreen reputation builds up automatically. The prompt stops appearing once the file hash has sufficient download
  history"; Microsoft suggests submitting the signed file to Microsoft Security Intelligence if prompts continue. Confidence **high**.
- **Failure modes.** Country ineligibility; identity validation failure after three document attempts ("we recommend that you delete
  your Artifact Signing account so that you aren't billed"); the account cannot move between subscriptions or tenants; suspension on
  misuse.

## 3. OV or individual (IV) certificate from a certificate authority

Sources: `https://knowledge.digicert.com/alerts/code-signings-new-private-key-storage-requirement` (last modified Nov 10, 2026),
`https://www.digicert.com/signing/code-signing-certificates`, `https://www.ssl.com/certificates/code-signing/`,
`https://shop.certum.eu/open-source-code-signing.html`, `https://cabforum.org/working-groups/code-signing/documents/` (current Baseline
Requirements v3.11, text only as PDF, not read); read Nov 11, 2026. Sectigo and GlobalSign pages were not read.

- **The hardware rule.** "Starting June 1, 2023, at 00:00 UTC, industry standards will require private keys for code signing
  certificates to be stored on hardware certified as FIPS 140-2 Level 2, Common Criteria EAL 4+, or equivalent" (DigiCert, quoting the
  CA/Browser Forum rule; "This requirement affects new, renewal, and reissue code signing certificate requests"). Delivery options at
  DigiCert: a shipped token (USD 120), the buyer's own token, an HSM, or the KeyLocker cloud service. Confidence **high** for the rule
  (vendor statement of the forum rule; the forum PDF itself was not read).
- **Prices read.** DigiCert: "Starting at $44 / month / certificate", 12-month subscription USD 696 (the page does not label which block
  is OV; the next block, USD 972, is presumably EV); KeyLocker price not shown. Certum "Open Source Code Signing - set": EUR 69 including a
  cryptoCertum card and a reader; the certificate carries "natural person data prefixed with "Open Source Developer" phrase"; validity
  period not stated on the page; the cloud variant page (SimplySign) returned 404. SSL.com: IV ("Personal name on every installer. No
  business docs required"), OV ("Business entity required"), EV; prices behind "Buy" links; keys on "a FIPS 140-2 validated hardware
  token, eSigner cloud HSM, or supported cloud HSM"; eSigner price not shown; its FAQ says EV and OV "are treated equally since March
  2024" for SmartScreen. Confidence **medium** (prices read once, pages partly inconsistent).
- **Individuals and country.** Certum's open-source set and SSL.com's IV are explicitly for natural persons; the publisher shown is the
  person's legal name. Which countries each CA validates was not on the pages read (**low**; ask the CA before paying).
- **CI fit.** A card or token must be plugged into the signing machine: GitHub-hosted runners cannot use it, so either a self-hosted
  runner on the maintainer's hardware (against the project's "no hardware in CI" habit) or **signing by hand on release day** (the
  maintainer already notarizes the macOS DMGs locally; PE files can be signed on macOS with `jsign` (Apache-2.0) or `osslsigncode` (GPL,
  a tool only, nothing ships) through PKCS#11, or on the lab ThinkPad with `signtool`). Cloud signing (KeyLocker, eSigner, SimplySign)
  removes the card but adds a subscription and its own secrets, and whether an unattended CI run is allowed without a phone approval was
  not confirmed (**low**). Confidence **high** for the card limitation.
- **SmartScreen.** As every non-Store route: "Valid Certificate (OV/EV): Warning — app flagged as unrecognized until reputation
  accumulates; verified publisher name is displayed". Confidence **high**.
- **Failure modes.** Yearly renewal with re-validation; a lost or broken card means re-issue; the certificate is in the maintainer's
  name (privacy); a change of certificate restarts publisher reputation.

## 4. EV certificate

Sources as in section 3 plus `https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation` (ms.date May 4,
2026). "EV certificates no longer bypass SmartScreen. Years ago, signing files with an Extended Validation (EV) code signing certificate
would result in positive SmartScreen reputation by default, but this behavior no longer exists. EV certificates may matter for enterprise
procurement, but they no longer impact SmartScreen behavior. Paying a premium for EV solely to avoid SmartScreen warnings is no longer
justified." DigiCert lists USD 972 a year; an organisation is normally required (SSL.com offers "EV SP" for sole proprietors). Hardware
token or cloud HSM as in 3. **Not an option for this project.** Confidence **high**.

## 5. Microsoft Store (MSIX)

Sources: `https://learn.microsoft.com/en-us/windows/apps/publish/partner-center/open-a-developer-account` (ms.date Oct 29, 2026),
`https://blogs.windows.com/windowsdeveloper/2025/09/10/free-developer-registration-for-individual-developers-on-microsoft-store/`,
`https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements` (updated Aug 24, 2026),
`https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/app-package-requirements`,
`https://learn.microsoft.com/en-us/windows/apps/publish/store-policies` (version 7.20, ms.date Sep 14, 2026),
`https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-prepare` (updated Jun 10, 2025),
`https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation`; read Nov 11, 2026.

- **Account.** "With the new onboarding experience, there are no registration fees for either account type"; the new flow starts only at
  `https://storedeveloper.microsoft.com`; an Individual account needs a personal Microsoft account, "identity verification with a
  government-issued ID and selfie"; free individual registration is "globally available in nearly 200 markets worldwide" (Windows
  Developer Blog, Sep 10, 2025). An Individual account cannot later become a Company account. The older fee table (which listed Vietnam
  at 2,000,000 VND) is superseded by the no-fee statement on the current page. Confidence **high** for "free"; **medium** for "available
  in the maintainer's country" (the market list was not read; confirm at sign-up).
- **Signing by the Store (MSIX only).** "Your MSIX and AppX packages don't have to be signed with a certificate rooted in a trusted
  certificate authority when submitting to the Microsoft Store. The Microsoft Store will automatically re-sign your MSIX/AppX packages
  with a Microsoft certificate"; no CA certificate, token or HSM needed. **The EXE or MSI route does not get this:** "If you are
  submitting an MSI or EXE installer to the Store, the Store does not re-sign those files. You must Authenticode-sign your MSI/EXE
  installer yourself" and policy 10.2.9 requires that "The binary and all of its Portable Executable (PE) files must be digitally signed
  with a code signing certificate that chains up to a certificate issued by a Certificate Authority (CA) that is part of the Microsoft
  Trusted Root Program" plus silent install and a stable versioned URL. So the Store helps only with an MSIX. Confidence **high**.
- **SmartScreen effect.** "Store-distributed apps are signed by a Microsoft certificate and are never subject to SmartScreen download
  warnings." This is for the Store install path; the GitHub installer download keeps its warning. Confidence **high**.
- **Packaging changes for an MSIX of CaptureMann** (from the "prepare" page): no writes to the install folder, no HKLM, HKCU and AppData
  virtualised per app (the library and settings live under the user profile today: check the paths the app uses), no elevation ("apps
  that require elevation for any part of their functionality won't be accepted"), no drivers or per-user services, no in-process shell
  extensions, must install silently, must work on Windows 10 S ("Avoid starting command utilities such as PowerShell and Cmd.exe": the
  NSIS `path.ps1` step disappears; the CLI becomes an `appExecutionAlias`), the Store-reserved fourth version number. Not on the page:
  whether a low-level mouse hook (`WH_MOUSE_LL`, used for the cursor track) and screen capture pass certification; nothing in the
  policies read forbids a screen recorder, and 10.5.1 requires a privacy policy URL ("If your product accesses, collects or transmits
  Personal Information ... you must maintain a privacy policy"). Confidence **high** for the listed rules, **low** for "a screen recorder
  with a mouse hook passes certification".
- **Cost in work.** A second package format (manifest, `makeappx`, `appExecutionAlias` for `capturemann` and `cm-mcp`, bundled FFmpeg and
  llama-server inside the package), Partner Center submission per release, certification wait, and a separate update channel. It does
  not replace the GitHub installer (the winget manifest and the direct download stay).
- **Failure modes.** Certification rejection; account verification failure; the Store identity (publisher) must match the manifest for
  later packages; Store packages cannot be installed from a GitHub download without the maintainer signing them himself.

## 6. winget alone

Sources: `https://learn.microsoft.com/en-us/windows/package-manager/package/windows-package-manager-policies` (document version 1.0, May
22, 2021), `https://learn.microsoft.com/en-us/windows/package-manager/package/repository` (updated Jul 14, 2026),
`https://github.com/microsoft/winget-pkgs/blob/master/README.md`, `https://github.com/microsoft/winget-cli/blob/master/doc/Settings.md`;
read Nov 11, 2026.

- The policies require no code signature. Validation of a pull request: schema, "The installer and application are virus free", installs
  and uninstalls silently for administrators and non-administrators, "The installer comes directly from the publisher's website", the
  URL reputation test ("URL-Validation-Error ... or the URL reputation test failed"), Microsoft Defender dynamic scan and "several
  antivirus programs" (Binary-Validation-Error), manual moderator review; "Microsoft reserves the right to refuse a submission for any
  reason". Confidence **high**.
- **What it changes:** nothing for a browser download; a winget install skips the browser and so Edge's panel. **Whether the installer
  winget downloads and runs carries the mark of the web and triggers "Windows protected your PC"** is not stated on any Microsoft page
  read (`Settings.md` has no SmartScreen or MotW entry). The sentence in `packaging/windows/winget/README.md` that a winget install
  "normally does not show the SmartScreen warning" is therefore **unverified**; keep it out of the release text until a report in
  `verification/` shows it. Confidence **low**.
- State: pull requests 472327 (0.2.8) and 477195 (0.3.1) were still open on Nov 10, 2026; the route has not yet delivered a listing.

## 7. Do nothing and document the warning

What the user sees (0.3.1, `verification/L1-windows-0.3.1-download-smartscreen-edge-2026-11-11-maintainer.md`): Edge's download panel
says the file "isn't commonly downloaded" and holds it as `Unconfirmed ... .crdownload`; the user must open the "..." menu, choose Keep,
then "Show more" and "Keep anyway" (wording as reported in Microsoft Q&A threads, not in a Learn article; **medium**). Running the kept
file then shows Windows' "Windows protected your PC" dialog (SmartScreen in the shell, for files with the mark of the web) with
"Unknown publisher", "More info", "Run anyway" (**high**, Microsoft's reputation page: "No signature: Warning — 'Windows protected your
PC'; User must choose 'Run anyway'"). Edge's panel is a SmartScreen **AppRep** download warning (the Edge policy
`ExemptSmartScreenDownloadWarnings` names it "SmartScreen AppRep based warnings", ms.date Oct 1, 2026); it is separate from Edge's
file-type interruptions ("was blocked", for downloads without a user gesture) and from Chrome, which uses Google Safe Browsing (not
researched here). On Windows 11 with Smart App Control on, "Smart App Control will block execution of unsigned files unless the file has
a positive reputation" and the user cannot run it at all (**high**).

**Release-text paragraph (unsigned case, replaces the one in `packaging/windows/winget/README.md`):**

> **Windows download warnings.** The Windows files are not code-signed yet (free signing for open source is being arranged). Edge
> may say the installer "isn't commonly downloaded": open the download's menu, choose Keep, then Show more and Keep anyway. When you
> run it, Windows may show "Windows protected your PC": choose More info, then Run anyway. Before you do, check the file's SHA-256
> against `SHA256SUMS.txt` on this page (`certutil -hashfile <file> SHA256`). On a PC with Smart App Control turned on the installer
> cannot run; use the portable zip or wait for a signed release.

## How reputation builds and what to tell users

From `https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation` (ms.date May 4, 2026), confidence **high**:

- Two signals: "Publisher reputation — Is the file signed? Is the signing certificate from a known, trusted publisher?" and "File hash
  reputation — Has this specific file been downloaded by users without indications of malicious behavior?"
- "Even when signed, a newly created binary could still show a SmartScreen warning until its hash or publisher certificate accumulates
  sufficient evidence of positive reputation."
- "When a file is not signed, SmartScreen reputation must build for each new version of your files, starting with zero reputation.
  Reputation cannot transfer from previous versions unless both were signed using the same publisher identity." This is the real gain of
  signing for a project that releases every few days: the certificate carries reputation from one version to the next.
- "There is no exact threshold, but it can take several weeks and hundreds of clean installs from a wide audience." "There is no need
  (or mechanism) to manually submit a file for SmartScreen reputation review for consumer endpoints." (The Artifact Signing FAQ and the
  SmartScreen overview nevertheless point to `https://www.microsoft.com/wdsi/filesubmission` for warnings believed wrong; **medium**.)
- Practice: "Sign every release", "Do not modify signed files" (zip after signing, never re-pack a signed installer), "Use a consistent
  signing identity", "Communicate with early adopters — for new apps, let beta users know they may see a SmartScreen prompt on first
  download".

**Release-text paragraph (signed case):**

> **Windows signature.** The installer, the uninstaller and the CaptureMann programs are code-signed. Free code signing provided by
> SignPath.io, certificate by SignPath Foundation: the publisher shown by Windows is "SignPath Foundation", which signs releases built
> by this repository's public GitHub Actions workflow from the tagged source (our code signing policy: <link>). The bundled FFmpeg and
> llama-server are upstream programs and are not signed. Windows SmartScreen still warns about programs it has not seen often, signed or
> not; the warning fades as more people install the same signed release. If you see "Windows protected your PC", check that the
> publisher reads SignPath Foundation, then choose More info and Run anyway.

## Does the NSIS installer need changes?

- **Stable publisher name.** The installer's registry `Publisher` is "CaptureMann contributors" and the signature's publisher will be
  "SignPath Foundation" (or the maintainer's name under option 3); the two are shown in different places (Apps list and the
  SmartScreen dialog) and need not match, but both must stay constant from release to release. Add `VIAddVersionKey "CompanyName"` so the
  installer's properties show a company string. **Medium** (NSIS mechanics high; Microsoft's "consistent signing identity" advice
  covers the certificate, not the registry string).
- **Version resources in the executables.** Required by SignPath's terms ("Product name metadata must match the project name"; "Product
  version metadata must be identical across each build") and shown by Windows in the file's properties and in SmartScreen's dialog.
  Needs a `build.rs` (for example the `winresource` crate) for `cm-desktop`, `cm-overlay`, `cm-cli`, `cm-mcp`, `cm-daemon`, with
  `ProductName` "CaptureMann", `FileVersion` and `ProductVersion` from `CARGO_PKG_VERSION`, `CompanyName`, `LegalCopyright`,
  `FileDescription`, and the icon that `packaging/windows/README.md` lists as not done. **High**.
- **Sign the files inside before packaging.** `pe-file` is not a container, so the five executables are signed first (as one zip
  artifact), then copied back into the staged tree before `makensis` and before `Compress-Archive`. **High**.
- **Sign the uninstaller.** Two-pass build per the NSIS wiki (`EXPORT_UNINST` / `IMPORT_UNINST`, `!uninstfinalize` copies the
  generated `uninstall.exe` out; NSIS 3.08 or later; confirm with `makensis /VERSION` on the runner). Without it the uninstaller is the
  only unsigned CaptureMann executable; it runs from disk without the mark of the web, so SmartScreen does not warn, but Smart App
  Control and SignPath's "sign your own binaries" expectation argue for signing it. **High**.
- **Timestamp every signature** so it outlives the certificate (SignPath handles this; with Artifact Signing it is mandatory because
  the certificate lives three days). **High**.
- **Check before publishing:** `signtool verify /pa /v` on `setup.exe`, `uninstall.exe` (extracted by a silent install) and the five
  executables, in the workflow, failing the run on any unsigned CaptureMann file. **High**.

## Implementation outline for the recommended route (SignPath Foundation)

Not started; research only. Order of work once the application is accepted.

**Maintainer (accounts and decisions, nothing of this is automatable):**

1. Apply at `https://signpath.org/apply` (repository URL, licence, releases page, the GitHub accounts of the team with 2FA on, contact).
   Ask in the application: expected review time; whether the FFmpeg and llama-server builds made by the workflow may be signed or must
   stay unsigned; whether one request may carry the installer and uninstaller together.
2. Publish the **Code signing policy** page (new `docs/project/code-signing-policy.md`, linked from the README and the release text) with
   the exact sentence "Free code signing provided by SignPath.io, certificate by SignPath Foundation", the roles (Author: the maintainer;
   Reviewer and Approver: the maintainer, plus anyone else on the team), the build description (public `windows-package.yml` on
   GitHub-hosted runners from a tagged commit), the privacy statement (no telemetry; network traffic only for Google Drive, YouTube,
   model downloads and remote AI keys the user turns on; links to the Google and Hugging Face policies), the uninstall note, and the
   system changes the installer offers (Start Menu entry, HKCU uninstall entry, opt-in PATH, opt-in Open-with).
3. In SignPath: accept the terms, create the project "capturemann", artifact configurations (a `zip-file` with five nested `pe-file`
   entries for the programs; a `pe-file` for `uninstall.exe`; a `pe-file` for `setup.exe`; `description` "CaptureMann" and
   `description-url` the repository), a `test-signing` policy (automatic) and a `release-signing` policy (manual approval by the
   maintainer), a CI user with submitter rights and its API token.
4. Install the SignPath GitHub App on `craft-mann/capturemann`. Add the repository secret `SIGNPATH_API_TOKEN`; add repository variables
   (not secrets) `SIGNPATH_ORGANIZATION_ID`, `SIGNPATH_PROJECT_SLUG`. Never write these into files.
5. Approve each release's signing requests in the SignPath web UI when the workflow pauses (one approval per request).

**Repository changes (a later implementation task, with its own worktree and tests):**

1. `apps/*/build.rs` and `crates/cm-cli`, `cm-mcp`, `cm-daemon` `build.rs`: Windows version resource and icon (`target_os` stays inside
   `build.rs`, which is a build script, but check `scripts/check_target_os.py` treats it as allowed).
2. `packaging/windows/installer.nsi`: `VIAddVersionKey "CompanyName"`, the `EXPORT_UNINST` / `IMPORT_UNINST` blocks from the NSIS wiki
   pattern (`WriteUninstaller` or `File "uninstall.exe"`; `!uninstfinalize 'copy /Y "%1" ...'` under `EXPORT_UNINST`).
3. `packaging/windows/build-windows.ps1`: a `-SignedBinaries <dir>` switch (or `CM_SIGNED_BINARIES`) that stages the signed executables
   instead of `target\release`; a `-UninstallerOnly` pass and a `-Uninstaller <signed file>` pass for the two `makensis` runs; the zip
   written from the signed tree; a final `signtool verify /pa /v` over every CaptureMann executable when signing was requested
   (`CM_REQUIRE_SIGNATURE=1`).
4. `.github/workflows/windows-package.yml` (all on `windows-latest`; `permissions: contents: read` stays; add `actions: read` only if
   SignPath's docs require it for this repository):
   - build as today, then `actions/upload-artifact` (v4 or later, pinned by commit like the others) of `unsigned-binaries.zip` (the five
     executables) → `signpath/github-action-submit-signing-request@<pinned commit>` with `api-token: ${{ secrets.SIGNPATH_API_TOKEN }}`,
     `organization-id: ${{ vars.SIGNPATH_ORGANIZATION_ID }}`, `project-slug: ${{ vars.SIGNPATH_PROJECT_SLUG }}`,
     `signing-policy-slug: ${{ startsWith(github.ref, 'refs/tags/v') && 'release-signing' || 'test-signing' }}`,
     `github-artifact-id: ${{ steps.up1.outputs.artifact-id }}`, `wait-for-completion: true`, `output-artifact-directory: signed/bin`;
   - `build-windows.ps1` pass 1 (stage signed binaries, export the uninstaller) → upload `uninstall.exe` → sign → pass 2 (embed the signed
     uninstaller, build `setup.exe`, write the zip) → upload `setup.exe` → sign → copy the signed installer over the staged one, recompute
     the `.sha256` files **after** signing;
   - the existing tests (portable zip, silent install and uninstall) run on the signed files; add `signtool verify /pa /v` of
     `setup.exe`, the installed `uninstall.exe` and the five executables, and `Get-AuthenticodeSignature` output in the log;
   - a `workflow_dispatch` input `sign: true|false` so an unsigned package can still be built when SignPath is unavailable; the tag run
     uses `release-signing` and stops until approved (set `timeout-minutes` on that step).
5. `packaging/windows/release-files.sh`: unchanged in function; add a comment that the `.sha256` files are those of the signed files, and
   (optional, when `osslsigncode` is installed on the Mac) `osslsigncode verify` of the installer as a sanity check; the winget installer
   manifest needs no `SignatureSha256` (that field is for MSIX).
6. `packaging/windows/README.md` and `packaging/windows/winget/README.md`: replace the O4 wording; the release text templates get the
   signed-case paragraph above.
7. `docs/development/release-day-0.3.2.md`: the maintainer's approval step, and the download check below.

**Clean-PC download check to store in `verification/`** (template in `verification/README.md`; name
`L1-windows-0.3.2-download-smartscreen-<browser>-<date>-<who>.md`): on a Windows 11 PC or VM that has never seen the file (the lab
ThinkPad after deleting earlier downloads, or a fresh Windows Sandbox), with Smart App Control state noted: download the installer from
the release page in Edge; screenshot the download panel (warning or none, exact wording); Keep; run; screenshot the "Windows protected
your PC" dialog and record the publisher line (expected "SignPath Foundation"); More info, Run anyway; install; then `signtool verify
/pa /v` (or `Get-AuthenticodeSignature`) on `setup.exe`, the installed `uninstall.exe` and the five executables, with the timestamp;
Properties → Digital Signatures screenshot; repeat the download in Chrome and note its wording; repeat the whole check two to four weeks
later on another clean PC to record whether the warning has gone. Status PARTIAL unless both browsers and both dates are covered.

## Open questions for the maintainer

1. **Country of residence and whether a legal entity exists.** This decides Artifact Signing (US or Canada for individuals) and the CA
   routes; SignPath's terms have no country rule, but confirm during the application.
2. **Is "SignPath Foundation" acceptable as the publisher name** shown by Windows, instead of the maintainer's or the project's name?
   (Options 2 and 3 would show the maintainer's legal name and city.)
3. **Team for SignPath:** who are the Authors, Reviewers and Approvers; all need 2FA on GitHub and SignPath. A one-person team is allowed
   by the terms as read ("a team member trusted by the entire team"), but confirm with SignPath.
4. **Privacy statement wording** for the code signing policy page (Google Drive and YouTube when connected, model downloads from Hugging
   Face, remote AI keys): the maintainer signs this statement.
5. **Should the Store MSIX channel be started in parallel** (free account, ID plus selfie, MSIX work) or only if SignPath refuses?
6. **FFmpeg and llama-server:** leave unsigned (SignPath's default for upstream software) or ask to sign them as our own builds?
7. **Release gating:** the tag build will wait for a human approval in SignPath; is that acceptable on release day (it mirrors the
   macOS notarization step the maintainer already runs by hand)?
8. **Reputation measurement:** who runs the clean-PC check at release and again two to four weeks later, and on which machine?

## Not confirmed (re-check before relying on it)

- SignPath Foundation's approval time, the exact application form, whether one certificate is shared by all projects, the timestamp
  authority it uses, and whether a directory artifact may hold the installer and uninstaller together.
- Artifact Signing's price (pricing page showed placeholders; USD 9.99 a month comes from another Microsoft page).
- Certum's open-source certificate validity period and the price or existence of its cloud (SimplySign) variant for open source; SSL.com
  and Sectigo prices; which countries each CA validates individuals from; whether their cloud signing allows unattended CI.
- Whether winget-installed files carry the mark of the web (so whether winget avoids "Windows protected your PC").
- Chrome's wording and behaviour for the same download (Safe Browsing, not SmartScreen).
- Whether a screen recorder with a low-level mouse hook passes Microsoft Store certification as an MSIX, and whether the maintainer's
  market is among the Store's "nearly 200".
- The Edge "Keep anyway" click path wording (from Q&A threads, not a Learn article).
