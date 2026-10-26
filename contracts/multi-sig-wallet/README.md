# `multi-sig-wallet` — Authorization & Access-Control Rule

## Live Authorization Requirement (No Cached Authorization)

1. **Mandatory Live Cross-Contract Check**: Every privileged entry point in `multi-sig-wallet` **must** query `contracts/access-control` live (`require_role(caller, Role::...)` for `MULTISIG_COSIGNER`) at execution time. Storing or trusting a local boolean authorization flag across invocations is strictly prohibited.
2. **Epoch-Bound Cache Invalidation**: If authorization is ever memoized within a bounded context, it must use `access_control::RevocationAwareCache` tied to `access-control`'s `RevocationEvent` stream and monotonic `revocation_epoch` so that cached state cannot outlive a revocation.
