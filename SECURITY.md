# Security

This module reads files that can be hostile. A file that makes the module or a host do one of these things is a vulnerability:

- Read or write memory outside the module's instance.
- Stop or crash the host.
- Go past the memory cap or the time budget of a read.
- Give a payload that the file does not contain.

A pixel read takes pixels that the caller decoded with its own decoder. That decoder runs outside the module. A fault in that decoder is not a vulnerability in this module.

## Report a vulnerability

Use [GitHub private vulnerability reporting](https://github.com/Verifiabl/verifiabl-extraction/security/advisories/new). Do not open a public issue.

Include:

- The SHA-256 of the module, from `dist/verifiabl-extract.json`.
- The host and its runtime.
- An input file that causes the problem, if you have one. Use only synthetic data. Do not send real payslips.

We reply in the private advisory.
