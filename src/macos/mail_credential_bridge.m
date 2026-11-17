// Replaceable bridge. All new storage is owned by the separately installed,
// immutable vault. Retained legacy readers are used only for exact migration.
__attribute__((used)) static const char bridge_protocol_marker[] = "QINGYU_CREDENTIAL_BRIDGE_VAULT1";
static NSString *broker_path(BOOL development) {
    return [credential_directory() stringByAppendingPathComponent:development ? @"mail-client-development" : @"mail-client-installed"];
}
static NSString *broker_identifier(BOOL development) { return development ? @"com.qingyu.macos.debug" : @"com.qingyu.macos"; }
static OSStatus broker_call(BOOL development, unsigned char action, NSData *account, NSData *password, NSMutableData **output) {
    return credential_exchange(broker_path(development), broker_identifier(development), action, account, password, output);
}

#ifdef QINGYU_CREDENTIAL_BROKER
static OSStatus vault_call(unsigned char action, NSData *account, NSData *password, NSMutableData **output) {
    return credential_exchange([credential_directory() stringByAppendingPathComponent:@"qingyu-mail-vault-v1"], @"com.qingyu.macos.credentials.vault-v1", action, account, password, output);
}

static OSStatus legacy_load(BOOL development, BOOL allow, NSData *account, NSMutableData **output) {
    NSString *directory = [credential_directory() stringByAppendingPathComponent:@"legacy-readers"];
    NSArray *names = [[[NSFileManager defaultManager] contentsOfDirectoryAtPath:directory error:nil] sortedArrayUsingSelector:@selector(compare:)];
    OSStatus last = errSecItemNotFound;
    NSString *interactive_reader = nil;
    // Try every retained exact owner silently first. This never falls back
    // from a denied newer service to an older value in a different service.
    for (NSString *name in names) {
        if (![name hasPrefix:@"reader-"]) continue;
        NSString *path = [directory stringByAppendingPathComponent:name];
        NSData *binary = [NSData dataWithContentsOfFile:path];
        if ([binary rangeOfData:[@"mail-client-development" dataUsingEncoding:NSUTF8StringEncoding] options:0 range:NSMakeRange(0, binary.length)].location != NSNotFound) continue;
        interactive_reader = path;
        OSStatus status = credential_exchange(path, @"com.qingyu.macos.debug.credentials", 0, account, nil, output);
        if (!status) return status;
        if (status != errSecItemNotFound) last = status;
    }
    if (allow && interactive_reader) {
        // The newest raw reader implements Installed -> Development ordering;
        // an old debug-only reader cannot authenticate an installed caller.
        last = credential_exchange(interactive_reader, @"com.qingyu.macos.debug.credentials", 1, account, nil, output);
    }
    return last;
}

static OSStatus broker_load(BOOL development, BOOL allow, NSData *account, NSMutableData **output) {
    OSStatus status = vault_call(allow ? 1 : 0, account, nil, output);
    if (status != errSecItemNotFound) return status;
    // An existing Installed value always wins. Even authorization failure on
    // that item must not select an older Development/original password.
    OSStatus installed = development ? errSecItemNotFound : item_exists("Qingyu.Mail.Installed", account);
    if (installed != errSecItemNotFound && installed != errSecSuccess) return installed;
    if (!development && installed == errSecItemNotFound) {
        status = broker_call(YES, allow ? 6 : 5, account, nil, output);
    } else status = legacy_load(development, allow, account, output);
    if (status) return status;
    NSMutableData *ignored = nil;
    status = vault_call(4, account, *output, &ignored);
    // Verify in a fresh vault process. A one-time legacy Allow is converted to
    // an item owned by the vault before reporting durable credential success.
    clear_secret(*output); *output = nil;
    if (!status) status = vault_call(0, account, nil, output);
    return status;
}

int main(void) {
    @autoreleasepool {
        signal(SIGPIPE, SIG_IGN); SecKeychainSetUserInteractionAllowed(false);
        pid_t parent = getppid(); BOOL bridge = !peer_process(parent, @"com.qingyu.macos.debug.credentials");
        BOOL installed_peer = !bridge && !peer_process(parent, @"com.qingyu.macos");
        if (parent <= 1 || (!bridge && !installed_peer) || getppid() != parent) return 77;
        SecCodeRef own = NULL; SecRequirementRef debug = NULL;
        SecCodeCopySelf(0, &own); SecRequirementCreateWithString(CFSTR("identifier \"com.qingyu.macos.debug\""), 0, &debug);
        BOOL development = !SecCodeCheckValidity(own, kSecCSStrictValidate, debug);
        if (own) CFRelease(own); if (debug) CFRelease(debug);
        unsigned char action; NSMutableData *account = nil, *input = nil, *output = nil;
        OSStatus status = receive_request(&action, &account, &input);
        if (status || action > 6 || (installed_peer && (!development || action < 5))) return send_response(errSecParam, nil);
        if (action == 0 || action == 1 || action == 5 || action == 6) status = broker_load(development, action == 1 || action == 6, account, &output);
        else status = vault_call(action, account, input, &output);
        clear_secret(input); int result = send_response(status, output); clear_secret(output); return result;
    }
}
#else
static OSStatus app_parent(BOOL *development) {
    pid_t parent = getppid(); OSStatus status = peer_process(parent, @"com.qingyu.macos.debug");
    if (!status) *development = YES;
    else { status = peer_process(parent, @"com.qingyu.macos"); if (!status) *development = NO; }
    return parent > 1 && getppid() == parent ? status : errSecAuthFailed;
}

// The unattended maintenance entry only copies existing Keychain values to
// vault-owned items; it cannot return secrets, change passwords, or request UI.
static int migrate_existing(void) {
    SecKeychainSetUserInteractionAllowed(false);
    OSStatus status;
    // Account lists contain identities only. Limit maintenance to configured
    // accounts so old disposable tests/unrelated items never affect readiness.
    NSMutableSet *accounts = [NSMutableSet set];
    NSMutableArray *paths = [NSMutableArray arrayWithObject:[NSHomeDirectory() stringByAppendingPathComponent:@"Library/Application Support/Qingyu/mail-accounts.json"]];
    NSDictionary *environment = NSProcessInfo.processInfo.environment;
    if ([environment[@"QINGYU_DATA_DIR"] length]) [paths addObject:[environment[@"QINGYU_DATA_DIR"] stringByAppendingPathComponent:@"mail-accounts.json"]];
    if ([environment[@"QINGYU_DEV_ACCOUNT_FILE"] length]) [paths addObject:environment[@"QINGYU_DEV_ACCOUNT_FILE"]];
    for (NSString *path in paths) {
        NSData *raw = [NSData dataWithContentsOfFile:path];
        id rows = raw ? [NSJSONSerialization JSONObjectWithData:raw options:0 error:nil] : nil;
        if (![rows isKindOfClass:NSArray.class]) continue;
        for (id row in rows) {
            if (![row isKindOfClass:NSDictionary.class] || [row[@"enabled"] isEqual:@NO]) continue;
            NSString *account = [row[@"email"] isKindOfClass:NSString.class] ? [row[@"email"] stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet].lowercaseString : nil;
            if (!account.length) continue;
            if ([row[@"provider"] isEqual:@"custom"]) {
                NSDictionary *custom = row[@"custom"];
                if (![custom isKindOfClass:NSDictionary.class]) continue;
                NSString *host = [custom[@"host"] stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet].lowercaseString;
                NSString *user = [custom[@"username"] stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
                if (!user.length) user = [row[@"email"] stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
                NSString *identity = [NSString stringWithFormat:@"%@%C%@%C%@%C%@", account, (unichar)0, host, (unichar)0, custom[@"port"], (unichar)0, user];
                NSData *bytes = [identity dataUsingEncoding:NSUTF8StringEncoding]; unsigned char digest[CC_SHA256_DIGEST_LENGTH];
                CC_SHA256(bytes.bytes, (CC_LONG)bytes.length, digest); NSMutableString *key = [NSMutableString stringWithString:@"custom-"];
                for (size_t i = 0; i < sizeof(digest); i++) [key appendFormat:@"%02x", digest[i]];
                account = key;
            }
            [accounts addObject:account];
        }
    }
    unsigned failed = 0;
    for (NSString *account in accounts) {
        NSData *key = [account dataUsingEncoding:NSUTF8StringEncoding];
        for (NSNumber *context in @[@YES, @NO]) {
            NSMutableData *password = nil; status = broker_call(context.boolValue, 0, key, nil, &password);
            if (status && status != errSecItemNotFound) failed++;
            clear_secret(password);
        }
    }
    printf("{\"accounts_checked\":%lu,\"unavailable_contexts\":%u}\n", (unsigned long)accounts.count, failed);
    return failed ? 2 : 0;
}

int main(int argc, char **argv) {
    @autoreleasepool {
        signal(SIGPIPE, SIG_IGN); SecKeychainSetUserInteractionAllowed(false);
        if (argc == 2 && !strcmp(argv[1], "--migrate-existing")) return migrate_existing();
        BOOL development = NO; OSStatus status = app_parent(&development); if (status) return 77;
        unsigned char action; NSMutableData *account = nil, *input = nil, *output = nil;
        status = receive_request(&action, &account, &input);
        if (!status && action > 4) status = errSecParam;
        if (!status) status = broker_call(development, action, account, input, &output);
        clear_secret(input); int result = send_response(status, output); clear_secret(output); return result;
    }
}
#endif
