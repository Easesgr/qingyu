// App-only startup verification. Keep this outside mail_keychain.m so ordinary
// bootstrap changes never rebuild the persistent Keychain helper.
#import <Foundation/Foundation.h>
#import <Security/Security.h>

extern int32_t qingyu_mail_verify_dev_helper(const char *path);

int32_t qingyu_dev_startup_ready(const char *helper_path) {
    @autoreleasepool {
        SecCodeRef running = NULL;
        SecRequirementRef requirement = NULL;
        OSStatus status = SecCodeCopySelf(kSecCSDefaultFlags, &running);
        if (!status) status = SecRequirementCreateWithString(CFSTR("identifier \"com.qingyu.macos.debug\""), kSecCSDefaultFlags, &requirement);
        // Validate the actual mapped process, not just a replacement file that
        // another build may already have put at current_exe().
        if (!status) status = SecCodeCheckValidity(running, kSecCSStrictValidate, requirement);
        if (!status) status = qingyu_mail_verify_dev_helper(helper_path);
        if (requirement) CFRelease(requirement);
        if (running) CFRelease(running);
        return status;
    }
}
