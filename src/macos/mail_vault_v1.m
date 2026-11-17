// IMMUTABLE storage protocol v1. Install once; never recompile/replace the
// installed vault for app, bridge, compiler or SDK updates. A future storage
// version must migrate through this retained owner before changing identity.
#import <Foundation/Foundation.h>
#import <Security/Security.h>
#import <CommonCrypto/CommonDigest.h>
#include <fcntl.h>
#include <errno.h>
#include "mail_credential_identity.h"
#include "mail_credential_protocol.h"

int main(void) {
    @autoreleasepool {
        signal(SIGPIPE, SIG_IGN);
        pid_t parent = getppid(); BOOL development = !peer_process(parent, @"com.qingyu.macos.debug");
        if (!development && peer_process(parent, @"com.qingyu.macos")) return 77;
        if (parent <= 1 || getppid() != parent) return 77;
        unsigned char action; NSMutableData *account = nil, *input = nil, *output = nil;
        OSStatus status = receive_request(&action, &account, &input);
        if (status || action > 4) return send_response(errSecParam, nil);
        status = SecKeychainSetUserInteractionAllowed(action == 1 || action == 2 || action == 3);
        const char *service = development ? "Qingyu.Mail.Vault.v1.Development" : "Qingyu.Mail.Vault.v1.Installed";
        if (!status && (action == 0 || action == 1)) {
            UInt32 length = 0; void *data = NULL;
            status = SecKeychainFindGenericPassword(NULL, (UInt32)strlen(service), service, (UInt32)account.length, account.bytes, &length, &data, NULL);
            if (!status) output = [NSMutableData dataWithBytes:data length:length];
            if (data) SecKeychainItemFreeContent(NULL, data);
        } else if (!status && (action == 2 || action == 4)) {
            // Only save may update; migration/import is always add-only.
            status = SecKeychainAddGenericPassword(NULL, (UInt32)strlen(service), service, (UInt32)account.length, account.bytes, (UInt32)input.length, input.bytes, NULL);
            if (status == errSecDuplicateItem && action == 4) status = errSecSuccess;
            else if (status == errSecDuplicateItem && action == 2) {
                SecKeychainItemRef item = NULL; status = item_reference(service, account, &item);
                if (!status) status = SecKeychainItemModifyAttributesAndData(item, NULL, (UInt32)input.length, input.bytes);
                if (item) CFRelease(item);
            }
        } else if (!status && action == 3) {
            SecKeychainItemRef item = NULL; status = item_reference(service, account, &item);
            if (!status) status = SecKeychainItemDelete(item);
            if (status == errSecItemNotFound) status = errSecSuccess;
            if (item) CFRelease(item);
        }
        clear_secret(input); int result = send_response(status, output); clear_secret(output); return result;
    }
}
