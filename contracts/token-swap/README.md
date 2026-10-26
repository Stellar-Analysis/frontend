# `token-swap` — Authorization & Access-Control Rule

## Live Authorization Requirement (No Cached Authorization)

1. **Mandatory Live Cross-Contract Check**: Every privileged entry point in `token-swap` **must** query `contracts/access-control` live (`require_role(caller, Role::...)` for `SWAP_OPERATOR`) at execution time. Storing or trusting a local boolean authorization flag across invocations is strictly prohibited.
2. **Epoch-Bound Cache Invalidation**: If authorization is ever memoized within a bounded context, it must use `access_control::RevocationAwareCache` tied to `access-control`'s `RevocationEvent` stream and monotonic `revocation_epoch` so that cached state cannot outlive a revocation.
