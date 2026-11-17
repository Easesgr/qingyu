// Private pipe protocol shared by the immutable vault and replaceable bridge.
#include <arpa/inet.h>
#include <libproc.h>
#include <signal.h>
#include <spawn.h>
#include <sys/wait.h>
#include <unistd.h>
extern char **environ;

static OSStatus peer_process(pid_t pid, NSString *identifier) {
    SecRequirementRef requirement = NULL; SecCodeRef code = NULL;
    OSStatus status = requirement_for_peer(identifier, &requirement);
    NSDictionary *attributes = @{ (__bridge NSString *)kSecGuestAttributePid: @(pid) };
    if (!status) status = SecCodeCopyGuestWithAttributes(NULL, (__bridge CFDictionaryRef)attributes, kSecCSDefaultFlags, &code);
    if (!status) status = SecCodeCheckValidity(code, kSecCSStrictValidate, requirement);
    if (code) CFRelease(code); if (requirement) CFRelease(requirement);
    return status;
}

static OSStatus peer_file(NSString *path, NSString *identifier) {
    SecRequirementRef requirement = NULL; SecStaticCodeRef code = NULL;
    OSStatus status = requirement_for_peer(identifier, &requirement);
    NSURL *url = [NSURL fileURLWithPath:path];
    if (!status) status = SecStaticCodeCreateWithPath((__bridge CFURLRef)url, kSecCSDefaultFlags, &code);
    if (!status) status = SecStaticCodeCheckValidity(code, kSecCSStrictValidate, requirement);
    if (code) CFRelease(code); if (requirement) CFRelease(requirement);
    return status;
}

static NSString *credential_directory(void) {
    char path[PROC_PIDPATHINFO_MAXSIZE];
    if (proc_pidpath(getpid(), path, sizeof(path)) <= 0) return nil;
    return [[NSString stringWithUTF8String:path] stringByDeletingLastPathComponent];
}

static BOOL pipe_read(int fd, void *data, size_t count) {
    while (count) { ssize_t n = read(fd, data, count); if (n < 0 && errno == EINTR) continue; if (n <= 0) return NO; data = (char *)data + n; count -= n; }
    return YES;
}
static BOOL pipe_write(int fd, const void *data, size_t count) {
    while (count) { ssize_t n = write(fd, data, count); if (n < 0 && errno == EINTR) continue; if (n <= 0) return NO; data = (const char *)data + n; count -= n; }
    return YES;
}

static OSStatus receive_request(unsigned char *action, NSMutableData **account, NSMutableData **password) {
    uint32_t lengths[2];
    if (!pipe_read(0, action, 1) || !pipe_read(0, lengths, sizeof(lengths))) return errSecParam;
    UInt32 a = ntohl(lengths[0]), p = ntohl(lengths[1]);
    if (!a || a > 4096 || p > 65536 || (*action != 2 && *action != 4 && p)) return errSecParam;
    *account = [NSMutableData dataWithLength:a]; *password = [NSMutableData dataWithLength:p];
    return pipe_read(0, (*account).mutableBytes, a) && pipe_read(0, (*password).mutableBytes, p) ? errSecSuccess : errSecParam;
}

static int send_response(OSStatus status, NSData *password) {
    if (password.length > 65536) status = errSecParam;
    uint32_t response[] = { htonl((uint32_t)status), htonl(status ? 0 : (UInt32)password.length) };
    BOOL ok = pipe_write(1, response, sizeof(response));
    if (!status && password.length) ok = ok && pipe_write(1, password.bytes, password.length);
    return ok ? 0 : 74;
}

static OSStatus credential_exchange(NSString *path, NSString *identifier, unsigned char action, NSData *account, NSData *password, NSMutableData **output) {
    OSStatus status = peer_file(path, identifier); if (status) return status;
    int input[2], result[2];
    if (pipe(input)) return errSecIO;
    if (pipe(result)) { close(input[0]); close(input[1]); return errSecIO; }
    for (int i = 0; i < 2; i++) { fcntl(input[i], F_SETFD, FD_CLOEXEC); fcntl(result[i], F_SETFD, FD_CLOEXEC); }
    posix_spawn_file_actions_t actions; posix_spawn_file_actions_init(&actions);
    posix_spawn_file_actions_adddup2(&actions, input[0], STDIN_FILENO);
    posix_spawn_file_actions_adddup2(&actions, result[1], STDOUT_FILENO);
    posix_spawn_file_actions_addclose(&actions, input[1]); posix_spawn_file_actions_addclose(&actions, result[0]);
    char *argv[] = { (char *)path.fileSystemRepresentation, NULL }; pid_t child = -1;
    int spawn = posix_spawn(&child, argv[0], &actions, NULL, argv, environ);
    posix_spawn_file_actions_destroy(&actions); close(input[0]); close(result[1]);
    if (spawn) { close(input[1]); close(result[0]); return errSecIO; }
    status = peer_process(child, identifier);
    uint32_t lengths[] = { htonl((UInt32)account.length), htonl((UInt32)password.length) };
    if (!status && (!pipe_write(input[1], &action, 1) || !pipe_write(input[1], lengths, sizeof(lengths)) || !pipe_write(input[1], account.bytes, account.length) || !pipe_write(input[1], password.bytes, password.length))) status = errSecIO;
    close(input[1]);
    uint32_t response[2];
    if (!status) {
        if (!pipe_read(result[0], response, sizeof(response))) status = errSecAuthFailed;
        else {
            status = (OSStatus)ntohl(response[0]); UInt32 length = ntohl(response[1]);
            if (length > 65536 || (status && length)) status = errSecParam;
            else if (!status) { *output = [NSMutableData dataWithLength:length]; if (!pipe_read(result[0], (*output).mutableBytes, length)) status = errSecIO; }
        }
    }
    close(result[0]); if (status) kill(child, SIGKILL);
    int exit_status; while (waitpid(child, &exit_status, 0) < 0 && errno == EINTR) {}
    if (!status && (!WIFEXITED(exit_status) || WEXITSTATUS(exit_status))) status = errSecIO;
    return status;
}

static OSStatus item_reference(const char *service, NSData *account, SecKeychainItemRef *item) {
    return SecKeychainFindGenericPassword(NULL, (UInt32)strlen(service), service, (UInt32)account.length, account.bytes, NULL, NULL, item);
}
static OSStatus item_exists(const char *service, NSData *account) {
    SecKeychainItemRef item = NULL; OSStatus status = item_reference(service, account, &item); if (item) CFRelease(item); return status;
}
static void clear_secret(NSMutableData *data) { if (data.length) memset_s(data.mutableBytes, data.length, 0, data.length); }
