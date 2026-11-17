// A persistent local Keychain boundary. The app and helper verify
// each other's certificate and identifier; secrets travel only through pipes.
#import <Foundation/Foundation.h>
#import <Security/Security.h>
#import <CommonCrypto/CommonDigest.h>
#include <arpa/inet.h>
#include <unistd.h>

#include "mail_credential_identity.h"

// Called by the Rust client before sending any account or password bytes.
int32_t qingyu_mail_verify_dev_helper(const char *path) {
    @autoreleasepool {
        SecRequirementRef requirement = NULL;
        SecStaticCodeRef helper = NULL;
        OSStatus status = requirement_for_peer(@"com.qingyu.macos.debug.credentials", &requirement);
        NSURL *url = [NSURL fileURLWithPath:[NSString stringWithUTF8String:path]];
        if (!status) status = SecStaticCodeCreateWithPath((__bridge CFURLRef)url, kSecCSDefaultFlags, &helper);
        if (!status) status = SecStaticCodeCheckValidity(helper, kSecCSStrictValidate, requirement);
        if (helper) CFRelease(helper);
        if (requirement) CFRelease(requirement);
        return status;
    }
}

int32_t qingyu_mail_verify_dev_helper_process(int32_t pid) {
    @autoreleasepool {
        SecRequirementRef requirement = NULL;
        SecCodeRef helper = NULL;
        OSStatus status = requirement_for_peer(@"com.qingyu.macos.debug.credentials", &requirement);
        NSDictionary *attributes = @{ (__bridge NSString *)kSecGuestAttributePid: @(pid) };
        if (!status) status = SecCodeCopyGuestWithAttributes(NULL, (__bridge CFDictionaryRef)attributes, kSecCSDefaultFlags, &helper);
        if (!status) status = SecCodeCheckValidity(helper, kSecCSStrictValidate, requirement);
        if (helper) CFRelease(helper);
        if (requirement) CFRelease(requirement);
        return status;
    }
}

#if defined(QINGYU_DEV_KEYCHAIN_HELPER) || defined(QINGYU_CREDENTIAL_BROKER)
#include <fcntl.h>
#include <errno.h>
#include "mail_credential_protocol.h"
#include "mail_credential_bridge.m"
#endif
