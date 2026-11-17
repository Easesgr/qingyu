// Certificate pinning for native credential peers.
static OSStatus requirement_for_peer(NSString *identifier, SecRequirementRef *requirement) {
    SecCodeRef own = NULL;
    CFDictionaryRef information = NULL;
    OSStatus status = SecCodeCopySelf(kSecCSDefaultFlags, &own);
    if (!status) status = SecCodeCheckValidity(own, kSecCSStrictValidate, NULL);
    if (!status) status = SecCodeCopySigningInformation(own, kSecCSSigningInformation, &information);
    NSArray *certificates = information ? ((__bridge NSDictionary *)information)[(__bridge NSString *)kSecCodeInfoCertificates] : nil;
    if (!status && !certificates.count) status = errSecAuthFailed;
    if (!status) {
        CFDataRef data = SecCertificateCopyData((__bridge SecCertificateRef)certificates.firstObject);
        if (!data) status = errSecAuthFailed;
        else {
            unsigned char digest[CC_SHA1_DIGEST_LENGTH];
            CC_SHA1(CFDataGetBytePtr(data), (CC_LONG)CFDataGetLength(data), digest);
            NSMutableString *fingerprint = [NSMutableString string];
            for (size_t i = 0; i < sizeof(digest); i++) [fingerprint appendFormat:@"%02x", digest[i]];
            NSString *text = [NSString stringWithFormat:@"identifier \"%@\" and certificate leaf = H\"%@\"", identifier, fingerprint];
            status = SecRequirementCreateWithString((__bridge CFStringRef)text, kSecCSDefaultFlags, requirement);
            CFRelease(data);
        }
    }
    if (information) CFRelease(information);
    if (own) CFRelease(own);
    return status;
}

