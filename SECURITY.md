# Security Policy

## Supported versions

The latest version of `happy-cracking` published to crates.io receives security updates.

## Reporting a vulnerability

Do not open a public GitHub issue for a security problem.

Report it through GitHub private vulnerability reporting:

https://github.com/riya-amemiya/happy-cracking/security/advisories/new

If that form is unavailable, email riya-amemiya+github@tokidux.com.

Include a description of the issue, steps to reproduce, affected versions, and
a proof of concept when you have one.

Please wait for a maintainer response before any public disclosure.

## What to report

Report defects in this project, including panics and memory-safety issues,
unbounded resource use on crafted input, incorrect cryptographic or encoding
results, and vulnerabilities in release artifacts or dependencies.

`happy-cracking` is a CTF toolkit. Hash cracking, textbook cryptosystem
attacks, port scanning, and HTTP load generation are intended commands. Use of
those commands against systems you do not control is outside this policy.
