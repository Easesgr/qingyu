#import <AppKit/AppKit.h>
#import <mach/mach.h>
#import <libproc.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>
#import <SystemConfiguration/SystemConfiguration.h>
#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <net/if.h>
#include <netinet/in.h>
#include <poll.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/mount.h>
#include <sys/stat.h>
#include <time.h>

// All returned strings are independently allocated; Rust releases them with qy_services_free.
void qy_services_free(void *value) { free(value); }

static uint32_t qy_physical_interface(NSString *name, NSArray *interfaces) {
    if (![name isKindOfClass:NSString.class]) return 0;
    for (id value in interfaces) {
        SCNetworkInterfaceRef interface = (__bridge SCNetworkInterfaceRef)value;
        if (![name isEqualToString:(__bridge NSString *)SCNetworkInterfaceGetBSDName(interface)]) continue;
        CFStringRef type = SCNetworkInterfaceGetInterfaceType(interface);
        // Select configured uplinks, never utun/PPP/IPsec or another VPN layer.
        if (type && (CFEqual(type, kSCNetworkInterfaceTypeEthernet) || CFEqual(type, kSCNetworkInterfaceTypeIEEE80211) ||
                     CFEqual(type, kSCNetworkInterfaceTypeBond) || CFEqual(type, kSCNetworkInterfaceTypeVLAN) ||
                     CFEqual(type, kSCNetworkInterfaceTypeFireWire))) return if_nametoindex(name.UTF8String);
    }
    return 0;
}

uint32_t qy_services_direct_interface(void) {
    @autoreleasepool {
        NSArray *interfaces = CFBridgingRelease(SCNetworkInterfaceCopyAll());
        SCDynamicStoreRef store = SCDynamicStoreCreate(NULL, CFSTR("Qingyu weather uplink"), NULL, NULL);
        if (!store) return 0;
        NSDictionary *global = CFBridgingRelease(SCDynamicStoreCopyValue(store, CFSTR("State:/Network/Global/IPv4")));
        uint32_t index = qy_physical_interface(global[@"PrimaryInterface"], interfaces);
        if (!index) {
            // Some VPNs become the primary service. Find the first connected
            // physical service in the user's existing network service order.
            NSDictionary *setup = CFBridgingRelease(SCDynamicStoreCopyValue(store, CFSTR("Setup:/Network/Global/IPv4")));
            for (NSString *service in setup[@"ServiceOrder"]) {
                NSString *key = [NSString stringWithFormat:@"State:/Network/Service/%@/IPv4", service];
                NSDictionary *state = CFBridgingRelease(SCDynamicStoreCopyValue(store, (__bridge CFStringRef)key));
                if (![state[@"Router"] length] || ![state[@"Addresses"] count]) continue;
                index = qy_physical_interface(state[@"InterfaceName"], interfaces);
                if (index) break;
            }
        }
        CFRelease(store);
        return index;
    }
}

static uint64_t qy_monotonic_millis(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (uint64_t)now.tv_sec * 1000 + now.tv_nsec / 1000000;
}

int qy_services_direct_connect(const char *address, uint16_t port, uint32_t interface, uint32_t timeout_ms) {
    // Reject an unavailable interface rather than allowing the default TUN route.
    if (!interface || !timeout_ms) { errno = ENETUNREACH; return -1; }
    struct sockaddr_storage storage = {0};
    struct sockaddr_in *v4 = (struct sockaddr_in *)&storage;
    struct sockaddr_in6 *v6 = (struct sockaddr_in6 *)&storage;
    int family;
    socklen_t length;
    if (inet_pton(AF_INET, address, &v4->sin_addr) == 1) {
        family = AF_INET; length = sizeof(*v4);
        v4->sin_family = family; v4->sin_len = length; v4->sin_port = htons(port);
    } else if (inet_pton(AF_INET6, address, &v6->sin6_addr) == 1) {
        family = AF_INET6; length = sizeof(*v6);
        v6->sin6_family = family; v6->sin6_len = length; v6->sin6_port = htons(port);
    } else { errno = EINVAL; return -1; }
    int socket_fd = socket(family, SOCK_STREAM, 0);
    if (socket_fd < 0) return -1;
    int yes = 1;
    int flags = fcntl(socket_fd, F_GETFL, 0);
    if (flags < 0 || fcntl(socket_fd, F_SETFD, FD_CLOEXEC) < 0 ||
        fcntl(socket_fd, F_SETFL, flags | O_NONBLOCK) < 0 ||
        setsockopt(socket_fd, SOL_SOCKET, SO_NOSIGPIPE, &yes, sizeof(yes)) < 0 ||
        setsockopt(socket_fd, family == AF_INET ? IPPROTO_IP : IPPROTO_IPV6,
            family == AF_INET ? IP_BOUND_IF : IPV6_BOUND_IF, &interface, sizeof(interface)) < 0) goto failed;
    if (connect(socket_fd, (struct sockaddr *)&storage, length) < 0) {
        if (errno != EINPROGRESS) goto failed;
        uint64_t deadline = qy_monotonic_millis() + timeout_ms;
        struct pollfd item = {.fd = socket_fd, .events = POLLOUT};
        for (;;) {
            uint64_t now = qy_monotonic_millis();
            if (now >= deadline) { errno = ETIMEDOUT; goto failed; }
            int ready = poll(&item, 1, (int)(deadline - now));
            if (ready < 0 && errno == EINTR) continue;
            if (ready <= 0) { if (!ready) errno = ETIMEDOUT; goto failed; }
            int error = 0;
            socklen_t size = sizeof(error);
            if (getsockopt(socket_fd, SOL_SOCKET, SO_ERROR, &error, &size) < 0) goto failed;
            if (error) { errno = error; goto failed; }
            break;
        }
    }
    if (fcntl(socket_fd, F_SETFL, flags) < 0) goto failed;
    return socket_fd;
failed:;
    int error = errno;
    close(socket_fd);
    errno = error;
    return -1;
}

// Low-level NSPasteboard bridge. All type policy, request selection and write plans live in Rust.
typedef struct {
    long change;
    uint32_t kind; // 0 unavailable, 1 success, 2 changeCount race, 3 exceeds requested bound
    void *data;
    size_t len;
    char *extension; // Reserved; always NULL.
} QYClipboardSnapshot;

static NSPasteboard *qy_pasteboard(const char *name) {
    return name ? [NSPasteboard pasteboardWithName:[NSString stringWithUTF8String:name]] : NSPasteboard.generalPasteboard;
}
static void qy_snapshot_data(QYClipboardSnapshot *result, NSData *data) {
    if (!data.length) return;
    result->data = malloc(data.length);
    if (result->data) { memcpy(result->data, data.bytes, data.length); result->len = data.length; }
}
static void qy_clipboard_check_change(NSPasteboard *pasteboard, long expected, QYClipboardSnapshot *result) {
    if (pasteboard.changeCount == expected) return;
    free(result->data); result->data = NULL; result->len = 0; result->kind = 2;
}

void qy_services_clipboard_metadata(const char *name, bool include_types, QYClipboardSnapshot *result) {
    @autoreleasepool {
        memset(result, 0, sizeof(*result));
        NSPasteboard *pasteboard = qy_pasteboard(name); result->change = pasteboard.changeCount;
        if (!include_types) return;
        NSMutableArray *items = [NSMutableArray array];
        for (NSPasteboardItem *item in pasteboard.pasteboardItems) [items addObject:item.types ?: @[]];
        NSDictionary *metadata = @{@"change":@(result->change), @"types":pasteboard.types ?: @[], @"items":items};
        NSData *data = [NSJSONSerialization dataWithJSONObject:metadata options:0 error:NULL];
        if (data.length && data.length <= 1024 * 1024) { result->kind = 1; qy_snapshot_data(result, data); }
        qy_clipboard_check_change(pasteboard, result->change, result);
    }
}

void qy_services_clipboard_read(const char *name, long expected, size_t index, const char *raw_type,
                               size_t limit, bool property_list, QYClipboardSnapshot *result) {
    @autoreleasepool {
        memset(result, 0, sizeof(*result)); result->change = expected;
        NSPasteboard *pasteboard = qy_pasteboard(name);
        if (pasteboard.changeCount != expected) { result->kind = 2; return; }
        NSString *type = raw_type ? [NSString stringWithUTF8String:raw_type] : nil;
        if (!type) return;
        id source = pasteboard;
        if (index != SIZE_MAX) {
            NSArray<NSPasteboardItem *> *items = pasteboard.pasteboardItems;
            if (index >= items.count) return;
            source = items[index];
        }
        // Do not invoke implicit type conversion or a provider for a type absent from this exact item.
        if (![[source types] containsObject:type]) { qy_clipboard_check_change(pasteboard, expected, result); return; }
        NSData *data = nil;
        if (property_list) {
            id value = [source propertyListForType:type];
            if (value && [NSJSONSerialization isValidJSONObject:value]) data = [NSJSONSerialization dataWithJSONObject:value options:0 error:NULL];
        } else { data = [source dataForType:type]; }
        if (data && data.length <= limit) { result->kind = 1; qy_snapshot_data(result, data); }
        else if (data.length > limit) { result->kind = 3; }
        qy_clipboard_check_change(pasteboard, expected, result);
    }
}

bool qy_services_local_media_file(const char *path) {
    struct stat file; struct statfs volume;
    // A filesystem primitive: avoid hydrating cloud placeholders or reading a mounted network file.
    return path && stat(path, &file) == 0 && S_ISREG(file.st_mode) && !(file.st_flags & SF_DATALESS) &&
        statfs(path, &volume) == 0 && (volume.f_flags & MNT_LOCAL);
}

@interface QYClipboardFileProvider : NSObject <NSPasteboardItemDataProvider>
@property(nonatomic, copy) NSString *path;
@property(nonatomic) NSUInteger maxBytes;
@end
@implementation QYClipboardFileProvider
- (void)pasteboard:(NSPasteboard *)pasteboard item:(NSPasteboardItem *)item provideDataForType:(NSPasteboardType)type {
    if (!qy_services_local_media_file(self.path.fileSystemRepresentation)) return;
    NSDictionary *attributes = [NSFileManager.defaultManager attributesOfItemAtPath:self.path error:NULL];
    if ([attributes[NSFileSize] unsignedLongLongValue] > self.maxBytes) return;
    NSData *data = [NSData dataWithContentsOfFile:self.path options:NSDataReadingMappedIfSafe error:NULL];
    if (data.length && data.length <= self.maxBytes) [item setData:data forType:type];
}
@end

bool qy_services_clipboard_write(const char *name, const void *bytes, size_t len) {
    @autoreleasepool {
        if (!bytes || !len || len > 1024 * 1024) return false;
        id plan = [NSJSONSerialization JSONObjectWithData:[NSData dataWithBytes:bytes length:len] options:0 error:NULL];
        if (![plan isKindOfClass:NSDictionary.class]) return false;
        id descriptions = plan[@"items"];
        if (![descriptions isKindOfClass:NSArray.class] || ![descriptions count] || [descriptions count] > 64) return false;
        NSMutableArray<NSPasteboardItem *> *items = [NSMutableArray array];
        for (id description in descriptions) {
            if (![description isKindOfClass:NSDictionary.class]) return false;
            NSPasteboardItem *item = [NSPasteboardItem new];
            id representations = description[@"representations"];
            if (![representations isKindOfClass:NSArray.class]) return false;
            for (id representation in representations) {
                if (![representation isKindOfClass:NSDictionary.class]) return false;
                NSString *type = representation[@"type"]; id text = representation[@"text"], encoded = representation[@"data"];
                if (![type isKindOfClass:NSString.class]) return false;
                if ([text isKindOfClass:NSString.class]) { if (![item setString:text forType:type]) return false; }
                else if ([encoded isKindOfClass:NSString.class]) {
                    NSData *data = [[NSData alloc] initWithBase64EncodedString:encoded options:0];
                    if (!data || ![item setData:data forType:type]) return false;
                } else { return false; }
            }
            id lazy = description[@"lazy"];
            if (lazy) {
                if (![lazy isKindOfClass:NSDictionary.class] || ![lazy[@"path"] isKindOfClass:NSString.class] ||
                    ![lazy[@"type"] isKindOfClass:NSString.class] || ![lazy[@"max_bytes"] isKindOfClass:NSNumber.class]) return false;
                NSString *path = lazy[@"path"]; unsigned long long limit = [lazy[@"max_bytes"] unsignedLongLongValue];
                if (!limit || !qy_services_local_media_file(path.fileSystemRepresentation)) return false;
                NSDictionary *attributes = [NSFileManager.defaultManager attributesOfItemAtPath:path error:NULL];
                if (![attributes[NSFileSize] unsignedLongLongValue] || [attributes[NSFileSize] unsignedLongLongValue] > limit) return false;
                QYClipboardFileProvider *provider = [QYClipboardFileProvider new]; provider.path = path; provider.maxBytes = (NSUInteger)limit;
                if (![item setDataProvider:provider forTypes:@[lazy[@"type"]]]) return false;
            }
            [items addObject:item];
        }
        NSPasteboard *pasteboard = qy_pasteboard(name);
        // Publish all prepared items together. Rust decides their types, ordering and replay metadata.
        [pasteboard clearContents]; return [pasteboard writeObjects:items];
    }
}

static NSMutableDictionary<NSString *, NSNumber *> *qy_test_read_counts(void) {
    static NSMutableDictionary *counts; static dispatch_once_t once;
    dispatch_once(&once, ^{ counts = [NSMutableDictionary dictionary]; }); return counts;
}
@interface QYCountingClipboardTestProvider : NSObject <NSPasteboardItemDataProvider>
@property(nonatomic, copy) NSString *name;
@property(nonatomic) BOOL large;
@end
@implementation QYCountingClipboardTestProvider
- (void)pasteboard:(NSPasteboard *)pasteboard item:(NSPasteboardItem *)item provideDataForType:(NSPasteboardType)type {
    NSMutableDictionary *counts = qy_test_read_counts();
    @synchronized(counts) { counts[self.name] = @([counts[self.name] unsignedLongLongValue] + 1); }
    [item setData:self.large ? [NSMutableData dataWithLength:64 * 1024 * 1024] : [@"lazy fixture" dataUsingEncoding:NSUTF8StringEncoding] forType:type];
}
@end

// Test entry points always use uniquely named, private pasteboards, never the user's general pasteboard.
char *qy_services_test_pasteboard(void) {
    @autoreleasepool { return strdup(NSPasteboard.pasteboardWithUniqueName.name.UTF8String); }
}
void qy_services_test_pasteboard_release(const char *name) {
    @autoreleasepool {
        if (name) {
            NSMutableDictionary *counts = qy_test_read_counts();
            @synchronized(counts) { [counts removeObjectForKey:[NSString stringWithUTF8String:name]]; }
            [qy_pasteboard(name) releaseGlobally];
        }
    }
}

uint64_t qy_services_test_pasteboard_reads(const char *name) {
    @autoreleasepool {
        if (!name) return 0;
        NSMutableDictionary *counts = qy_test_read_counts();
        @synchronized(counts) { return [counts[[NSString stringWithUTF8String:name]] unsignedLongLongValue]; }
    }
}

bool qy_services_test_pasteboard_items(const char *name, const void *bytes, size_t len) {
    @autoreleasepool {
        if (!name || !bytes) return false;
        id descriptions = [NSJSONSerialization JSONObjectWithData:[NSData dataWithBytes:bytes length:len] options:0 error:NULL];
        if (![descriptions isKindOfClass:NSArray.class]) return false;
        NSMutableArray *items = [NSMutableArray array]; NSMutableArray *legacy = [NSMutableArray array];
        for (NSDictionary *description in descriptions) {
            NSPasteboardItem *item = [NSPasteboardItem new];
            if (description[@"text"]) [item setString:description[@"text"] forType:NSPasteboardTypeString];
            if (description[@"file"]) [item setString:[NSURL fileURLWithPath:description[@"file"]].absoluteString forType:NSPasteboardTypeFileURL];
            NSDictionary *values = description[@"values"];
            for (NSString *type in values) [item setString:values[type] forType:type];
            for (NSString *type in description[@"empty"]) [item setData:[NSData data] forType:type];
            NSArray *types = description[@"lazy"];
            if (types.count) {
                QYCountingClipboardTestProvider *provider = [QYCountingClipboardTestProvider new];
                provider.name = [NSString stringWithUTF8String:name]; provider.large = [description[@"large"] boolValue];
                if (![item setDataProvider:provider forTypes:types]) return false;
            }
            NSString *marker = description[@"marker"];
            if ([marker containsString:@" "]) [legacy addObject:marker];
            else if (marker) [item setData:[NSData data] forType:marker];
            [items addObject:item];
        }
        NSPasteboard *pasteboard = qy_pasteboard(name);
        [pasteboard clearContents]; BOOL written = [pasteboard writeObjects:items];
        if (written && legacy.count) { [pasteboard addTypes:legacy owner:nil]; for (NSString *type in legacy) [pasteboard setData:[NSData data] forType:type]; }
        return written;
    }
}

@interface QYChangingClipboardTestProvider : NSObject <NSPasteboardItemDataProvider>
@property(nonatomic, strong) NSData *data;
@end
@implementation QYChangingClipboardTestProvider
- (void)pasteboard:(NSPasteboard *)pasteboard item:(NSPasteboardItem *)item provideDataForType:(NSPasteboardType)type {
    [item setData:self.data forType:type];
    [pasteboard clearContents]; [pasteboard setString:@"changed during capture" forType:NSPasteboardTypeString];
}
@end

bool qy_services_test_pasteboard_write(const char *name, uint32_t kind, const void *bytes, size_t len, const char *marker) {
    @autoreleasepool {
        if (!name) return false;
        NSPasteboard *pasteboard = qy_pasteboard(name);
        NSData *data = [NSData dataWithBytes:bytes length:len];
        NSMutableArray<NSPasteboardItem *> *items = [NSMutableArray array];
        if (kind == 3) {
            id paths = [NSJSONSerialization JSONObjectWithData:data options:0 error:NULL];
            if (![paths isKindOfClass:NSArray.class]) return false;
            for (NSString *path in paths) {
                NSPasteboardItem *item = [NSPasteboardItem new];
                [item setString:[NSURL fileURLWithPath:path].absoluteString forType:NSPasteboardTypeFileURL];
                [item setString:path forType:NSPasteboardTypeString];
                [items addObject:item];
            }
        } else {
            NSPasteboardItem *item = [NSPasteboardItem new];
            if (kind == 6) {
                QYChangingClipboardTestProvider *provider = [QYChangingClipboardTestProvider new]; provider.data = data;
                [item setDataProvider:provider forTypes:@[NSPasteboardTypePNG]];
            } else if (kind == 2 || kind == 7) {
                [item setData:data forType:kind == 2 ? NSPasteboardTypePNG : NSPasteboardTypeTIFF];
                [item setString:@"image fallback text" forType:NSPasteboardTypeString];
            } else {
                [item setData:data forType:NSPasteboardTypeString];
            }
            [items addObject:item];
        }
        NSString *markerType = marker ? [NSString stringWithUTF8String:marker] : nil;
        BOOL legacyMarker = [markerType containsString:@" "];
        if (markerType && !legacyMarker && items.count) [items.lastObject setData:[NSData data] forType:markerType];
        [pasteboard clearContents]; BOOL written = [pasteboard writeObjects:items];
        if (written && legacyMarker) { [pasteboard addTypes:@[markerType] owner:nil]; [pasteboard setData:[NSData data] forType:markerType]; }
        return written;
    }
}

void *qy_services_test_pasteboard_data(const char *name, const char *type, size_t *len) {
    @autoreleasepool {
        *len = 0;
        if (!name || !type) return NULL;
        NSData *data = [qy_pasteboard(name) dataForType:[NSString stringWithUTF8String:type]];
        if (!data.length || data.length > 32 * 1024 * 1024) return NULL;
        void *copy = malloc(data.length);
        if (copy) { memcpy(copy, data.bytes, data.length); *len = data.length; }
        return copy;
    }
}

typedef struct { uint64_t physical, resident; uint32_t threads, descriptors; } QYMetrics;
bool qy_services_metrics(QYMetrics *result) {
    struct proc_taskinfo info = {0};
    if (proc_pidinfo(getpid(), PROC_PIDTASKINFO, 0, &info, sizeof(info)) != sizeof(info)) return false;
    result->resident = info.pti_resident_size;
    result->physical = result->resident;
    result->threads = info.pti_threadnum;
    int descriptors = proc_pidinfo(getpid(), PROC_PIDLISTFDS, 0, NULL, 0);
    result->descriptors = descriptors > 0 ? (uint32_t)(descriptors / sizeof(struct proc_fdinfo)) : 0;
    task_vm_info_data_t vm = {0};
    mach_msg_type_number_t count = TASK_VM_INFO_COUNT;
    if (task_info(mach_task_self(), TASK_VM_INFO, (task_info_t)&vm, &count) == KERN_SUCCESS)
        result->physical = vm.phys_footprint;
    return true;
}

#import <UserNotifications/UserNotifications.h>
void qy_services_notify(const char *title, const char *body, bool sound) {
    @autoreleasepool {
        // Command-line test runners have no notification identity; the island itself still shows reminders.
        if (!NSBundle.mainBundle.bundleIdentifier.length) return;
        NSString *heading = title ? [NSString stringWithUTF8String:title] : @"轻屿";
        NSString *message = body ? [NSString stringWithUTF8String:body] : @"";
        if (!heading || !message) return;
        UNUserNotificationCenter *center = UNUserNotificationCenter.currentNotificationCenter;
        void (^deliver)(void) = ^{
            UNMutableNotificationContent *content = [UNMutableNotificationContent new];
            content.title = heading; content.body = message;
            if (sound) content.sound = UNNotificationSound.defaultSound;
            UNNotificationRequest *request = [UNNotificationRequest requestWithIdentifier:NSUUID.UUID.UUIDString content:content trigger:nil];
            [center addNotificationRequest:request withCompletionHandler:^(NSError *error){}];
        };
        [center getNotificationSettingsWithCompletionHandler:^(UNNotificationSettings *settings){
            if (settings.authorizationStatus == UNAuthorizationStatusNotDetermined) {
                [center requestAuthorizationWithOptions:(UNAuthorizationOptionAlert | UNAuthorizationOptionSound)
                    completionHandler:^(BOOL granted, NSError *error){ if (granted) deliver(); }];
            } else if (settings.authorizationStatus == UNAuthorizationStatusAuthorized || settings.authorizationStatus == UNAuthorizationStatusProvisional) {
                deliver();
            }
        }];
    }
}

static uint32_t qy_color(NSColor *color, NSColor *surface) {
    color = [color colorUsingColorSpace:NSColorSpace.sRGBColorSpace];
    surface = [surface colorUsingColorSpace:NSColorSpace.sRGBColorSpace];
    CGFloat r=0,g=0,b=0,a=1,sr=1,sg=1,sb=1,sa=1;
    [color getRed:&r green:&g blue:&b alpha:&a];
    [surface getRed:&sr green:&sg blue:&sb alpha:&sa];
    return ((uint32_t)lround((r*a+sr*(1-a))*255)<<16) |
           ((uint32_t)lround((g*a+sg*(1-a))*255)<<8) |
            (uint32_t)lround((b*a+sb*(1-a))*255);
}
void qy_services_palette(bool dark, uint32_t *colors) {
    @autoreleasepool {
        BOOL contrast=NSWorkspace.sharedWorkspace.accessibilityDisplayShouldIncreaseContrast;
        NSAppearanceName name=contrast?(dark?NSAppearanceNameAccessibilityHighContrastDarkAqua:NSAppearanceNameAccessibilityHighContrastAqua):(dark?NSAppearanceNameDarkAqua:NSAppearanceNameAqua);
        NSAppearance *appearance = [NSAppearance appearanceNamed:name];
        [appearance performAsCurrentDrawingAppearance:^{
            NSColor *window = NSColor.windowBackgroundColor;
            NSColor *group = NSColor.controlBackgroundColor;
            NSArray<NSColor *> *roles = @[window, NSColor.underPageBackgroundColor, group,
                NSColor.controlColor, NSColor.labelColor, NSColor.secondaryLabelColor,
                NSColor.separatorColor, NSColor.controlAccentColor, NSColor.selectedContentBackgroundColor];
            for (NSUInteger i=0;i<roles.count;i++) colors[i] = qy_color(roles[i], group);
        }];
    }
}
uint32_t qy_services_preferences(void) {
    @autoreleasepool {
        NSWorkspace *workspace = NSWorkspace.sharedWorkspace;
        NSString *name = [NSApp.effectiveAppearance bestMatchFromAppearancesWithNames:@[NSAppearanceNameAqua, NSAppearanceNameDarkAqua]];
        return ([name isEqualToString:NSAppearanceNameDarkAqua] ? 1 : 0) |
            (workspace.accessibilityDisplayShouldReduceMotion ? 2 : 0) |
            (workspace.accessibilityDisplayShouldReduceTransparency ? 4 : 0) |
            (workspace.accessibilityDisplayShouldIncreaseContrast ? 8 : 0);
    }
}
