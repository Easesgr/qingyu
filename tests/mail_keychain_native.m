// Test service precedence without opening or changing a real Keychain item.
// xcrun clang -fobjc-arc -fblocks -Wno-deprecated-declarations \
//   -framework Foundation -framework Security tests/mail_keychain_native.m \
//   -o /tmp/qingyu-mail-keychain-test && /tmp/qingyu-mail-keychain-test
#import <Foundation/Foundation.h>
#import <Security/Security.h>

static NSMutableDictionary<NSString *, NSNumber *> *results;
static NSMutableArray<NSString *> *reads;
static NSUInteger writes;
static char synthetic[] = "synthetic-test-secret";

static OSStatus fixture_find(CFTypeRef keychain, UInt32 length, const char *name,
    UInt32 account_length, const char *account, UInt32 *size, void **data, SecKeychainItemRef *item) {
    NSString *service = [[NSString alloc] initWithBytes:name length:length encoding:NSUTF8StringEncoding];
    [reads addObject:service];
    OSStatus status = results[service] ? results[service].intValue : errSecItemNotFound;
    if (item) *item = NULL;
    if (!status) { if (size) *size = sizeof(synthetic)-1; if (data) *data = synthetic; }
    return status;
}
static OSStatus fixture_add(SecKeychainRef keychain, UInt32 length, const char *name,
    UInt32 account_length, const char *account, UInt32 size, const void *data, SecKeychainItemRef *item) {
    writes++; return errSecSuccess;
}
static OSStatus fixture_modify(SecKeychainItemRef item, const SecKeychainAttributeList *attributes, UInt32 size, const void *data) {
    writes++; return errSecSuccess;
}
#define SecKeychainFindGenericPassword fixture_find
#define SecKeychainAddGenericPassword fixture_add
#define SecKeychainItemModifyAttributesAndData fixture_modify
#define QINGYU_DEV_KEYCHAIN_HELPER 1
#define main credential_helper_main
#import "../src/macos/mail_keychain.m"
#undef main

static NSUInteger failures;
static void check(BOOL condition, const char *message) {
    if (!condition) { failures++; fprintf(stderr,"FAIL: %s\n",message); }
}
static OSStatus load(BOOL debug, NSDictionary<NSString *, NSNumber *> *statuses) {
    development = debug; service = debug ? "Qingyu.Mail.Development" : "Qingyu.Mail.Installed";
    results = statuses.mutableCopy; reads = [NSMutableArray new]; writes = 0;
    UInt32 length = 0; void *data = NULL;
    return load_secret("synthetic@example.invalid", 25, &length, &data);
}
int main(void) { @autoreleasepool {
    check(load(NO, @{@"Qingyu.Mail.Installed": @0}) == 0 && reads.count == 1 && !writes,
        "installed credential takes precedence without touching another service");
    check(load(NO, @{@"Qingyu.Mail.Development": @0}) == 0 && reads.count == 2 && !writes,
        "installed caller can reuse development value without copying or overwriting it");
    check(load(NO, @{@"Qingyu.Mail.Installed": @(errSecAuthFailed), @"Qingyu.Mail.Development": @0}) == errSecAuthFailed
        && reads.count == 1 && !writes, "denied installed credential cannot fall back to another identity");
    check(load(NO, @{@"Qingyu.Mail.Development": @(errSecInteractionNotAllowed), @"Qingyu.Mail": @0}) == errSecInteractionNotAllowed
        && reads.count == 2 && !writes, "denied development credential cannot fall back to legacy");
    check(load(NO, @{@"Qingyu.Mail": @0}) == 0 && reads.count == 3 && !writes,
        "installed legacy fallback preserves the original");
    check(load(YES, @{@"Qingyu.Mail.Installed": @0}) == errSecItemNotFound && ![reads containsObject:@"Qingyu.Mail.Installed"] && !writes,
        "development does not consume installed overrides");
    check(load(YES, @{@"Qingyu.Mail": @0}) == 0 && writes == 1,
        "development keeps its existing legacy migration");
    fprintf(stdout,"mail service isolation: 7 checks; %lu failures; no real Keychain access\n",(unsigned long)failures);
    return failures ? 1 : 0;
} }
