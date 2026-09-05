---
title: "Terms of Service"
linkTitle: "Terms"
description: "Terms for Super Pro purchase, delivery, and support."
toc: true
---

**Last updated:** 2026-09-05  
**Contact:** support@ddl.sconts.com  
**Operator:** Project Super Team

## 1. Scope

These Terms apply to **Super Pro** (official paid plugins and license keys) — purchase, delivery, and related support.

The **Project Super** Community Edition on GitHub remains under its open-source licenses and is **not** governed by these commercial Terms.

## 2. What you buy

One Super Pro subscription unlocks official plugins on the same **`superd` / `super`** binaries you already run, typically including:

- **security** — API auth, RBAC, audit (required for licensed startup)
- **ui** — Dashboard
- **notify** — webhook notifications
- **isolation** — Linux cgroup limits (Linux)

After purchase you receive: a plugin archive for your platform, a signed license key, and a short config snippet (e.g. `[license].key` and `auth_secret`).

See the [feature matrix](/docs/07-editions/feature-matrix/); guide: [Get Super Pro](/go/pro/).

## 3. Checkout and payment

Checkout and payment are completed on a **third-party platform** (currently **Afdian**). Super does not process card or wallet payments itself. **That platform’s terms apply to payment, settlement, and refunds initiated through their checkout.** We receive only the order details needed to fulfill (see [Privacy Policy](/legal/privacy/)).

The public Afdian checkout linked from [Get Super Pro](/go/pro/) is currently an **open-source supporter tip** (**¥10 / month**). **Tips do not include** official plugins or a license key.

**Super Pro** licenses (when sold) use **annual coverage**: one annual payment maps to a **365-day** license term. After that term, you may **continue using** Pro plugins on the Super version scope signed into your key; renewal is for following newer releases beyond that scope (see [Get Super Pro](/go/pro/#license-version-coverage)). During the public beta, free **90-day** trials are offered via the [Super Pro Portal claim](https://platform.ddl.sconts.com/portal/claim?product=super-pro&plan=first-trials-001) page.

## 4. Delivery

After payment clears and required order notes are complete (display name, OS + arch, email), we aim to deliver within **24 hours** by email or platform message. Delivery is digital only.

## 5. License use

- The key is for your licensed use of official plugins with a **signed Super version scope**: an issued release line plus newer minor lines up to a maximum written into the key (see [Get Super Pro — License version coverage](/go/pro/#license-version-coverage)).
- **After the 365-day term ends, you can still use** official plugins on that same signed version scope (grants are retained by default). Renewal is required to follow Super releases beyond the maximum (or a new major), via a new key.
- Do not redistribute private plugin packages or license keys to third parties.
- We may refuse fulfillment or revoke a key for fraud, chargeback abuse, or material breach (a refunded key is void).
- Phase 1 uses offline signed keys plus local expiry / version checks; there is **no** always-on license server required to start.

## 6. Refunds

Payment and refund mechanics are governed first by **Afdian’s platform rules**. In addition:

- **Before** plugins and key are delivered: we will support a full refund via the platform where possible.
- **After** delivery: generally no refund; material fulfillment errors may be handled case by case.
- **Wrong monthly tip instead of a paid Super Pro order:** tips are sponsorship only and **do not** include a license key; contact **support@ddl.sconts.com** if you meant to purchase Pro when paid SKUs are offered.
- A refunded or charged-back order voids any associated license key.

## 7. Support and changes

Support is best-effort via **support@ddl.sconts.com**. We may change prices, checkout channels, or the official plugin set; material changes will be noted on [Get Super Pro](/go/pro/) or the docs site. Pro plugins require a signed key that authorizes your installed `superd` version.

## 8. Disclaimer and limitation of liability

Software is provided “as is.” To the maximum extent permitted by law, the operator is not liable for indirect, incidental, or lost-profit damages; total liability to you will not exceed the amounts you actually paid for Super Pro in the **12 months** before the claim.

You are responsible for how you deploy Super (bind address, secrets, network exposure). Community Edition has no API auth by default; licensed deployments must load the security plugin and configure it correctly.

## 9. Final interpretation

The developer reserves the final right of interpretation of these Terms.

## 10. Contact

Questions about these Terms: **support@ddl.sconts.com**.
