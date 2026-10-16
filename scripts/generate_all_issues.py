#!/usr/bin/env python3
"""
Generate 70 GitHub issues for Stellar Insights Mobile & Multi-Network Architecture
"""

from pathlib import Path
import subprocess
import time

PROJECT_ROOT = Path(__file__).resolve().parent.parent
REQUIRED_SOURCE_ROOTS = ("backend/src", "sdk/src", "mobile/src")


def require_source_layout():
    """Stop before creating issues for source roots absent from this checkout."""
    missing = [
        path for path in REQUIRED_SOURCE_ROOTS
        if not (PROJECT_ROOT / path).is_dir()
    ]
    if missing:
        raise SystemExit(
            "No issues created: this legacy generator requires backend, SDK, "
            "and mobile source roots in the same checkout. Missing: "
            + ", ".join(missing)
            + ". Route these tasks to their actual source repositories first."
        )


def create_issue(title, body, index, total):
    """Create a single GitHub issue"""
    try:
        print(f"[{index}/{total}] Creating: {title}")
        
        cmd = ["gh", "issue", "create", "--title", title, "--body", body]
        result = subprocess.run(cmd, cwd=PROJECT_ROOT, capture_output=True, text=True, timeout=30)
        
        if result.returncode == 0:
            print(f"✓ Created issue {index}\n")
            return True
        else:
            print(f"✗ Failed: {result.stderr}\n")
            return False
            
    except Exception as e:
        print(f"✗ Error: {str(e)}\n")
        return False

# PHASE 1: BACKEND REFACTORING (20 issues)
phase1_issues = [
    ("Network Context Middleware", "middleware/network_context.rs", "Extract X-Stellar-Network header", "3-5"),
    ("Database Schema Separation", "database.rs", "Separate testnet/mainnet databases", "8-12"),
    ("Network-Aware RPC Client", "rpc/network_router.rs", "Route RPC calls by network", "6-8"),
    ("Mobile Pagination Endpoints", "handlers/corridors.rs", "Cursor-based pagination", "5-7"),
    ("Field Selection Parameter", "middleware/field_selector.rs", "Filter response fields", "4-6"),
    ("Response Compression", "main.rs", "Gzip/brotli compression", "3-4"),
    ("ETag Caching Support", "middleware/etag.rs", "HTTP caching with ETags", "4-5"),
    ("Batch Endpoints", "handlers/batch.rs", "Batch multiple requests", "6-8"),
    ("Network Status Endpoint", "handlers/network_status.rs", "Network health endpoint", "3-4"),
    ("Rate Limiting by Client", "middleware/rate_limit.rs", "Different limits for web/mobile", "5-6"),
    ("JWT Token Refresh", "handlers/auth.rs", "Token refresh endpoint", "4-5"),
    ("SEP-10 for Mobile", "auth/sep10.rs", "Mobile-friendly SEP-10", "6-8"),
    ("Push Notification Registration", "handlers/notifications.rs", "Register FCM/APNS tokens", "4-5"),
    ("Push Notification Service", "services/push_notifications.rs", "Send push notifications", "8-10"),
    ("WebSocket Real-Time Updates", "websocket/mod.rs", "Real-time data sync", "10-12"),
    ("API Versioning", "routes/mod.rs", "URL-based versioning", "5-6"),
    ("Deprecation Warnings", "middleware/deprecation.rs", "Deprecation headers", "3-4"),
    ("Mobile Request Logging", "middleware/logging.rs", "Enhanced mobile logging", "3-4"),
    ("Health Check Enhancement", "handlers/health.rs", "Detailed component status", "3-4"),
    ("Multi-Network Configuration", "config.rs", "Network-specific config", "4-5"),
]

# PHASE 2: SHARED SDK (15 issues)
phase2_issues = [
    ("Initialize SDK Package", "package.json", "TypeScript SDK foundation", "4-5"),
    ("API Client Core", "client/ApiClient.ts", "HTTP client with auth", "6-8"),
    ("Network Context Management", "network/NetworkManager.ts", "Network switching", "4-5"),
    ("Authentication Module", "auth/AuthManager.ts", "Token management", "8-10"),
    ("Retry with Backoff", "client/RetryManager.ts", "Exponential backoff", "4-5"),
    ("Request Deduplication", "client/RequestDeduplicator.ts", "Prevent duplicate requests", "3-4"),
    ("Request Cancellation", "client/ApiClient.ts", "AbortController support", "3-4"),
    ("TypeScript Types", "types/api.ts", "Complete type definitions", "6-8"),
    ("Corridors API Module", "api/corridors.ts", "Corridors SDK methods", "4-5"),
    ("Anchors API Module", "api/anchors.ts", "Anchors SDK methods", "4-5"),
    ("Assets API Module", "api/assets.ts", "Assets SDK methods", "3-4"),
    ("Analytics API Module", "api/analytics.ts", "Analytics SDK methods", "4-5"),
    ("React Native Compatibility", "client/ApiClient.ts", "RN environment support", "6-8"),
    ("SDK Unit Tests", "__tests__/", "Comprehensive tests", "8-10"),
    ("NPM Publishing Setup", ".github/workflows/publish.yml", "Automated publishing", "4-5"),
]

# PHASE 3: MOBILE APP (30 issues)
phase3_issues = [
    ("iOS Project Setup", "ios/Podfile", "Initialize iOS project", "6-8"),
    ("Android Project Setup", "android/build.gradle", "Initialize Android project", "6-8"),
    ("Splash Screen", "screens/SplashScreen.tsx", "Branded splash screen", "4-5"),
    ("Biometric Authentication", "services/biometrics.ts", "Face ID/Touch ID/Fingerprint", "6-8"),
    ("Secure Token Storage", "services/auth.ts", "Keychain storage", "4-5"),
    ("Login Screen", "screens/auth/LoginScreen.tsx", "Wallet connect login", "8-10"),
    ("Dashboard Screen", "screens/main/DashboardScreen.tsx", "Main dashboard with metrics", "10-12"),
    ("Corridors List", "screens/main/CorridorsScreen.tsx", "Corridors with search", "8-10"),
    ("Corridor Detail", "screens/main/CorridorDetailScreen.tsx", "Detailed corridor view", "8-10"),
    ("Anchors List", "screens/main/AnchorsScreen.tsx", "Anchors with status", "6-8"),
    ("Anchor Detail", "screens/main/AnchorDetailScreen.tsx", "Detailed anchor view", "6-8"),
    ("Settings Screen", "screens/main/SettingsScreen.tsx", "Settings with network switcher", "6-8"),
    ("Network Switch Dialog", "components/NetworkSwitchDialog.tsx", "Confirmation dialog", "3-4"),
    ("Offline Caching", "services/cache.ts", "MMKV-based caching", "6-8"),
    ("Offline Queue", "services/offlineQueue.ts", "Queue mutations offline", "8-10"),
    ("Network Status Indicator", "components/NetworkStatusBar.tsx", "Online/offline indicator", "4-5"),
    ("Pull-to-Refresh", "screens/main/", "Refresh gesture", "3-4"),
    ("Infinite Scroll", "hooks/useInfiniteScroll.ts", "Paginated lists", "5-6"),
    ("Search Functionality", "components/SearchBar.tsx", "Debounced search", "5-6"),
    ("Push Notifications Setup", "ios/GoogleService-Info.plist", "FCM configuration", "6-8"),
    ("Notification Preferences", "screens/main/NotificationSettingsScreen.tsx", "Notification settings", "4-5"),
    ("Sync Complete Notification", "services/offlineQueue.ts", "Local notification", "3-4"),
    ("Deep Linking", "navigation/linking.ts", "URL scheme handling", "5-6"),
    ("Share Functionality", "utils/share.ts", "Native share sheet", "3-4"),
    ("Dark Mode", "theme/ThemeProvider.tsx", "Dark theme support", "8-10"),
    ("Loading Skeletons", "components/skeletons/", "Skeleton loaders", "5-6"),
    ("Error Boundary", "components/ErrorBoundary.tsx", "Error handling", "4-5"),
    ("Sentry Integration", "services/sentry.ts", "Crash reporting", "4-5"),
    ("Performance Monitoring", "services/performance.ts", "Performance metrics", "6-8"),
    ("Analytics Tracking", "services/analytics.ts", "Event tracking", "5-6"),
]

# Additional issues to reach 70
additional_issues = [
    ("App Icon and Branding", "ios/Images.xcassets/", "App icons for iOS/Android", "4-5"),
    ("Haptic Feedback", "utils/haptics.ts", "Tactile feedback", "3-4"),
    ("Accessibility Improvements", "components/", "Screen reader support", "8-10"),
    ("Unit Tests for Services", "services/__tests__/", "Service layer tests", "10-12"),
    ("E2E Tests with Detox", "e2e/", "End-to-end testing", "12-15"),
]

def generate_body(title, file, description, effort):
    """Generate issue body"""
    return f"""## Project Context
Stellar Insights Mobile & Multi-Network Architecture

## Task Type
- [x] Feature

## File Locations
```
{file}
```

## Description
{description}

## Acceptance Criteria
- [ ] Implement core functionality
- [ ] Add error handling
- [ ] Write tests
- [ ] Update documentation

## Estimated Effort
{effort} hours"""

def main():
    require_source_layout()
    all_issues = []
    
    # Generate Phase 1 issues
    for i, (name, file, desc, effort) in enumerate(phase1_issues, 1):
        title = f"[Backend] {name}"
        body = generate_body(title, f"backend/src/{file}", desc, effort)
        all_issues.append((title, body))
    
    # Generate Phase 2 issues
    for i, (name, file, desc, effort) in enumerate(phase2_issues, 1):
        title = f"[SDK] {name}"
        body = generate_body(title, f"sdk/src/{file}", desc, effort)
        all_issues.append((title, body))
    
    # Generate Phase 3 issues
    for i, (name, file, desc, effort) in enumerate(phase3_issues, 1):
        title = f"[Mobile] {name}"
        body = generate_body(title, f"mobile/src/{file}", desc, effort)
        all_issues.append((title, body))
    
    # Generate additional issues
    for i, (name, file, desc, effort) in enumerate(additional_issues, 1):
        title = f"[Mobile] {name}"
        body = generate_body(title, f"mobile/{file}", desc, effort)
        all_issues.append((title, body))
    
    print(f"\n📋 Total issues to create: {len(all_issues)}\n")
    print("Creating GitHub issues...\n")
    
    success_count = 0
    
    for i, (title, body) in enumerate(all_issues, 1):
        if create_issue(title, body, i, len(all_issues)):
            success_count += 1
        
        # Pause every 10 issues
        if i % 10 == 0 and i < len(all_issues):
            print("⏸️  Pausing for 3 seconds...\n")
            time.sleep(3)
    
    print(f"\n✅ Done! Created {success_count}/{len(all_issues)} issues successfully.")

if __name__ == "__main__":
    main()
