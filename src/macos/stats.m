// Local macOS metrics, inspired by Stats (MIT), Copyright (c) 2019 Serhiy Mytrovtsiy.
// No permissions, subprocesses, external connections or persistent IOKit handles are used.
#import <Foundation/Foundation.h>
#import <IOKit/IOKitLib.h>
#import <IOKit/storage/IOBlockStorageDriver.h>
#import <SystemConfiguration/SystemConfiguration.h>
#import <mach/mach.h>
#import <mach/mach_time.h>
#include <ifaddrs.h>
#include <net/if.h>
#include <net/if_dl.h>
#include <net/route.h>
#include <netinet/in.h>
#include <sys/mount.h>
#include <sys/sysctl.h>
#include <errno.h>
#include <math.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

// The returned UTF-8 JSON is independently allocated and released by qy_stats_free.
void qy_stats_free(void *value) { free(value); }

static NSDictionary *QYDictionary(id value) {
    return [value isKindOfClass:NSDictionary.class] ? value : nil;
}

static NSNumber *QYNumber(id value) {
    if (![value isKindOfClass:NSNumber.class] || CFGetTypeID((__bridge CFTypeRef)value) != CFNumberGetTypeID()) return nil;
    return value;
}

static NSNumber *QYCounter(id value) {
    NSNumber *number = QYNumber(value);
    if (!number) return nil;
    // Avoid accepting floating-point statistics, negative signed counters or booleans as bytes.
    if (!strchr("csilqCSILQ", number.objCType[0]) || [number.stringValue hasPrefix:@"-"]) return nil;
    return number;
}

static NSString *QYString(id value) {
    if ([value isKindOfClass:NSString.class]) return value;
    if ([value isKindOfClass:NSData.class]) {
        NSString *text = [[NSString alloc] initWithData:value encoding:NSUTF8StringEncoding];
        return [[text stringByReplacingOccurrencesOfString:@"\0" withString:@""] stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
    }
    return nil;
}

static id QYProperty(io_registry_entry_t entry, CFStringRef key) {
    return CFBridgingRelease(IORegistryEntryCreateCFProperty(entry, key, kCFAllocatorDefault, 0));
}

static NSDictionary *QYStore(NSString *key) {
    return QYDictionary(CFBridgingRelease(SCDynamicStoreCopyValue(NULL, (__bridge CFStringRef)key)));
}

static uint64_t QYNanoseconds(uint64_t ticks, mach_timebase_info_data_t timebase) {
    return (uint64_t)(((__uint128_t)ticks * timebase.numer) / timebase.denom);
}

static NSString *QYCPUName(void) {
    char name[256] = {0}; size_t size = sizeof(name) - 1;
    if (sysctlbyname("machdep.cpu.brand_string", name, &size, NULL, 0) == 0 && name[0]) {
        name[sizeof(name) - 1] = 0;
        return [NSString stringWithUTF8String:name] ?: @"Mac CPU";
    }
    return @"Mac CPU";
}

static void QYReadHost(NSMutableDictionary *result, NSMutableArray<NSString *> *errors) {
    uint32_t logical = 0; size_t logicalSize = sizeof(logical);
    if (sysctlbyname("hw.logicalcpu", &logical, &logicalSize, NULL, 0) != 0) {
        long count = sysconf(_SC_NPROCESSORS_ONLN);
        if (count > 0 && count <= UINT32_MAX) logical = (uint32_t)count;
    }
    result[@"logical_cores"] = @(logical);
    result[@"cpu_name"] = QYCPUName();
    long ticks = sysconf(_SC_CLK_TCK);
    result[@"ticks_per_second"] = @(ticks > 0 ? (uint64_t)ticks : 100);

    mach_port_t host = mach_host_self();
    host_cpu_load_info_data_t cpu = {0};
    mach_msg_type_number_t cpuCount = HOST_CPU_LOAD_INFO_COUNT;
    if (host_statistics(host, HOST_CPU_LOAD_INFO, (host_info_t)&cpu, &cpuCount) == KERN_SUCCESS) {
        result[@"cpu_ticks"] = @[@(cpu.cpu_ticks[CPU_STATE_USER]), @(cpu.cpu_ticks[CPU_STATE_SYSTEM]),
            @(cpu.cpu_ticks[CPU_STATE_IDLE]), @(cpu.cpu_ticks[CPU_STATE_NICE])];
    } else { [errors addObject:@"CPU 计数不可用"]; }

    uint64_t total = 0; size_t totalSize = sizeof(total);
    vm_statistics64_data_t memory = {0};
    mach_msg_type_number_t memoryCount = HOST_VM_INFO64_COUNT;
    vm_size_t pageSize = 0;
    if (sysctlbyname("hw.memsize", &total, &totalSize, NULL, 0) == 0 && total > 0 &&
        host_page_size(host, &pageSize) == KERN_SUCCESS &&
        host_statistics64(host, HOST_VM_INFO64, (host_info64_t)&memory, &memoryCount) == KERN_SUCCESS) {
        // Stats' app+wired+compressed definition, excluding reclaimable and file-backed pages.
        uint64_t occupied = (uint64_t)memory.active_count + memory.inactive_count + memory.speculative_count +
            memory.wire_count + memory.compressor_page_count;
        uint64_t reclaimable = (uint64_t)memory.purgeable_count + memory.external_page_count;
        uint64_t usedPages = occupied > reclaimable ? occupied - reclaimable : 0;
        uint64_t used = usedPages * (uint64_t)pageSize;
        result[@"memory"] = @{@"total": @(total), @"used": @(MIN(used, total)),
            @"compressed": @((uint64_t)memory.compressor_page_count * pageSize)};
    } else { [errors addObject:@"内存统计不可用"]; }
    if (host != MACH_PORT_NULL) mach_port_deallocate(mach_task_self(), host);

    struct xsw_usage swap = {0}; size_t swapSize = sizeof(swap);
    if (sysctlbyname("vm.swapusage", &swap, &swapSize, NULL, 0) == 0) result[@"swap_used"] = @(swap.xsu_used);
    else [errors addObject:@"交换空间统计不可用"];
}

static void QYReadDisks(NSMutableDictionary *result, NSMutableArray<NSString *> *errors) {
    struct statfs space = {0};
    // One Data/startup volume avoids adding the same shared APFS container for each mount.
    if (statfs("/System/Volumes/Data", &space) == 0 || statfs("/", &space) == 0) {
        uint64_t block = space.f_bsize > 0 ? (uint64_t)space.f_bsize : 0;
        uint64_t total = space.f_blocks * block;
        uint64_t available = space.f_bavail * block;
        result[@"disk_space"] = @{@"total": @(total), @"available": @(MIN(available, total))};
    } else { [errors addObject:@"启动卷容量不可用"]; }

    io_iterator_t iterator = IO_OBJECT_NULL;
    if (IOServiceGetMatchingServices(kIOMainPortDefault, IOServiceMatching(kIOBlockStorageDriverClass), &iterator) != KERN_SUCCESS) {
        [errors addObject:@"磁盘 I/O 统计不可用"]; return;
    }
    NSMutableArray *devices = [NSMutableArray array];
    NSMutableSet<NSNumber *> *seen = [NSMutableSet set];
    BOOL complete = YES;
    io_registry_entry_t driver;
    while ((driver = IOIteratorNext(iterator)) != IO_OBJECT_NULL) {
        uint64_t identity = 0;
        NSDictionary *statistics = QYDictionary(QYProperty(driver, CFSTR(kIOBlockStorageDriverStatisticsKey)));
        NSNumber *read = QYCounter(statistics[@kIOBlockStorageDriverStatisticsBytesReadKey]);
        NSNumber *write = QYCounter(statistics[@kIOBlockStorageDriverStatisticsBytesWrittenKey]);
        if (IORegistryEntryGetRegistryEntryID(driver, &identity) == KERN_SUCCESS && read && write) {
            if (![seen containsObject:@(identity)]) {
                [seen addObject:@(identity)];
                [devices addObject:@{@"id": @(identity), @"read_bytes": read, @"write_bytes": write}];
            }
        } else { complete = NO; }
        IOObjectRelease(driver);
    }
    complete = complete && IOIteratorIsValid(iterator);
    IOObjectRelease(iterator);
    if (complete && devices.count) result[@"disk_io"] = devices;
    else [errors addObject:@"磁盘 I/O 统计不可用"];
}

static NSString *QYGPUName(io_registry_entry_t service, NSDictionary *properties, NSDictionary *statistics) {
    NSString *name = QYString(properties[@"model"]) ?: QYString(statistics[@"model"]);
    if (name.length) return name;
    io_registry_entry_t parent = IO_OBJECT_NULL;
    if (IORegistryEntryGetParentEntry(service, kIOServicePlane, &parent) == KERN_SUCCESS) {
        name = QYString(QYProperty(parent, CFSTR("model")));
        IOObjectRelease(parent);
        if (name.length) return name;
    }
    NSString *className = [QYString(properties[@"IOClass"]) lowercaseString];
    if ([className containsString:@"agx"]) return @"Apple Graphics";
    if ([className containsString:@"intel"]) return @"Intel Graphics";
    if ([className containsString:@"amd"]) return @"AMD Graphics";
    if ([className containsString:@"nvidia"] || [className containsString:@"nvaccelerator"]) return @"NVIDIA Graphics";
    return @"GPU";
}

static void QYReadGPUs(NSMutableDictionary *result, NSMutableArray<NSString *> *errors) {
    io_iterator_t iterator = IO_OBJECT_NULL;
    if (IOServiceGetMatchingServices(kIOMainPortDefault, IOServiceMatching("IOAccelerator"), &iterator) != KERN_SUCCESS) {
        [errors addObject:@"GPU 统计不可用"]; return;
    }
    NSMutableArray *gpus = [NSMutableArray array];
    io_registry_entry_t service;
    while ((service = IOIteratorNext(iterator)) != IO_OBJECT_NULL) {
        CFMutableDictionaryRef raw = NULL;
        if (IORegistryEntryCreateCFProperties(service, &raw, kCFAllocatorDefault, 0) == KERN_SUCCESS) {
            NSDictionary *properties = CFBridgingRelease(raw);
            NSDictionary *statistics = QYDictionary(properties[@"PerformanceStatistics"]);
            NSNumber *usage = QYNumber(statistics[@"Device Utilization %"]) ?: QYNumber(statistics[@"GPU Activity(%)"]);
            id percentage = NSNull.null;
            if (usage && isfinite(usage.doubleValue) && usage.doubleValue >= 0) percentage = @(MIN(usage.doubleValue, 100.0));
            [gpus addObject:@{@"name": QYGPUName(service, properties, statistics), @"usage": percentage}];
        } else { [errors addObject:@"部分 GPU 统计不可用"]; }
        IOObjectRelease(service);
    }
    if (!IOIteratorIsValid(iterator)) [errors addObject:@"GPU 设备列表已变化"];
    IOObjectRelease(iterator);
    result[@"gpus"] = gpus;
}

static BOOL QYPhysicalName(NSString *name) {
    // Ethernet, Wi-Fi and USB/Thunderbolt adapters use en*. Excludes lo, awdl,
    // llw, bridge and VPN tunnels, so a packet is never added once per virtual layer.
    return [name hasPrefix:@"en"] || [name hasPrefix:@"pdp_ip"];
}

static BOOL QYUsableAddress(const struct sockaddr *address) {
    if (!address) return NO;
    if (address->sa_family == AF_INET) {
        uint32_t ip = ntohl(((const struct sockaddr_in *)address)->sin_addr.s_addr);
        return ip != 0 && (ip >> 24) != 127 && (ip >> 16) != 0xa9fe;
    }
    if (address->sa_family == AF_INET6) {
        const struct in6_addr *ip = &((const struct sockaddr_in6 *)address)->sin6_addr;
        return !IN6_IS_ADDR_UNSPECIFIED(ip) && !IN6_IS_ADDR_LOOPBACK(ip) && !IN6_IS_ADDR_LINKLOCAL(ip);
    }
    return NO;
}

static NSString *QYPrimaryInterface(NSSet<NSString *> *candidates) {
    for (NSString *family in @[@"IPv4", @"IPv6"]) {
        NSString *primary = QYString(QYStore([@"State:/Network/Global/" stringByAppendingString:family])[@"PrimaryInterface"]);
        if ([candidates containsObject:primary]) return primary;
    }
    // With a VPN default route, prefer its physical underlay in system service order.
    for (NSString *family in @[@"IPv4", @"IPv6"]) {
        id order = QYStore([@"Setup:/Network/Global/" stringByAppendingString:family])[@"ServiceOrder"];
        if (![order isKindOfClass:NSArray.class]) continue;
        for (id serviceID in order) {
            if (![serviceID isKindOfClass:NSString.class]) continue;
            NSDictionary *service = QYStore([NSString stringWithFormat:@"State:/Network/Service/%@/%@", serviceID, family]);
            NSString *name = QYString(service[@"InterfaceName"]);
            if (service[@"Router"] && [candidates containsObject:name]) return name;
        }
    }
    return [[candidates allObjects] sortedArrayUsingSelector:@selector(localizedStandardCompare:)].firstObject;
}

static BOOL QYInterfaceCounters(unsigned int index, struct if_data64 *data) {
    int mib[] = {CTL_NET, PF_ROUTE, 0, 0, NET_RT_IFLIST2, (int)index};
    // Interfaces may appear during the size/read pair. Retry the bounded allocation once.
    for (int attempt = 0; attempt < 2; attempt++) {
        size_t size = 0;
        if (sysctl(mib, 6, NULL, &size, NULL, 0) != 0 || size == 0 || size > 16 * 1024 * 1024) return NO;
        void *buffer = malloc(size);
        if (!buffer) return NO;
        if (sysctl(mib, 6, buffer, &size, NULL, 0) != 0) {
            int failure = errno; free(buffer);
            if (failure == ENOMEM) continue;
            return NO;
        }
        size_t offset = 0; BOOL found = NO;
        // The first four bytes are common to route messages; copy to aligned stack storage.
        while (offset + 4 <= size) {
            uint16_t length = 0; memcpy(&length, (char *)buffer + offset, sizeof(length));
            uint8_t type = ((uint8_t *)buffer)[offset + 3];
            if (length < 4 || length > size - offset) break;
            if (type == RTM_IFINFO2 && length >= sizeof(struct if_msghdr2)) {
                struct if_msghdr2 message;
                memcpy(&message, (char *)buffer + offset, sizeof(message));
                if (message.ifm_index == index) { *data = message.ifm_data; found = YES; break; }
            }
            offset += length;
        }
        free(buffer); return found;
    }
    return NO;
}

static void QYReadNetwork(NSMutableDictionary *result, NSMutableArray<NSString *> *errors) {
    struct ifaddrs *addresses = NULL;
    if (getifaddrs(&addresses) != 0) {
        result[@"network_status"] = @"unavailable"; [errors addObject:@"网络接口不可用"]; return;
    }
    NSMutableSet<NSString *> *candidates = [NSMutableSet set];
    NSMutableDictionary<NSString *, NSString *> *hardware = [NSMutableDictionary dictionary];
    for (const struct ifaddrs *entry = addresses; entry; entry = entry->ifa_next) {
        if (!entry->ifa_name || !entry->ifa_addr) continue;
        NSString *name = [NSString stringWithUTF8String:entry->ifa_name];
        if (!name || !QYPhysicalName(name)) continue;
        unsigned flags = entry->ifa_flags;
        if ((flags & (IFF_UP | IFF_RUNNING)) != (IFF_UP | IFF_RUNNING) || (flags & (IFF_LOOPBACK | IFF_POINTOPOINT))) continue;
        if (QYUsableAddress(entry->ifa_addr)) [candidates addObject:name];
        if (entry->ifa_addr->sa_family == AF_LINK) {
            const struct sockaddr_dl *link = (const struct sockaddr_dl *)entry->ifa_addr;
            // Bounds include the inline name plus the link-layer address.
            if (link->sdl_alen > 0 && offsetof(struct sockaddr_dl, sdl_data) + link->sdl_nlen + link->sdl_alen <= link->sdl_len) {
                const unsigned char *bytes = (const unsigned char *)LLADDR(link);
                NSMutableString *mac = [NSMutableString string];
                for (unsigned i = 0; i < link->sdl_alen; i++) [mac appendFormat:@"%02x", bytes[i]];
                hardware[name] = mac;
            }
        }
    }
    freeifaddrs(addresses);
    NSString *name = QYPrimaryInterface(candidates);
    if (!name) { result[@"network_status"] = @"offline"; return; }
    unsigned int index = if_nametoindex(name.UTF8String);
    struct if_data64 data = {0};
    if (index == 0 || !QYInterfaceCounters(index, &data)) {
        result[@"network_status"] = @"unavailable"; [errors addObject:@"网络流量计数不可用"]; return;
    }
    NSString *identity = [NSString stringWithFormat:@"%@:%u:%@:%lld:%d", name, index, hardware[name] ?: @"",
        (long long)data.ifi_lastchange.tv_sec, (int)data.ifi_lastchange.tv_usec];
    result[@"network_status"] = @"online";
    result[@"network"] = @{@"interface": name, @"identity": identity,
        @"received_bytes": @(data.ifi_ibytes), @"sent_bytes": @(data.ifi_obytes)};
}

static char *QYJSON(NSDictionary *value) {
    NSData *data = [NSJSONSerialization dataWithJSONObject:value options:0 error:nil];
    if (!data) return NULL;
    char *result = malloc(data.length + 1);
    if (result) { memcpy(result, data.bytes, data.length); result[data.length] = 0; }
    return result;
}

char *qy_stats_sample_json(void) {
    @autoreleasepool {
        @try {
            NSMutableDictionary *result = [NSMutableDictionary dictionary];
            NSMutableArray<NSString *> *errors = [NSMutableArray array];
            mach_timebase_info_data_t timebase = {0}; mach_timebase_info(&timebase);
            if (timebase.denom) {
                // continuous includes sleep, absolute excludes it. Rust rejects intervals crossing sleep.
                result[@"continuous_ns"] = @(QYNanoseconds(mach_continuous_time(), timebase));
                result[@"awake_ns"] = @(QYNanoseconds(mach_absolute_time(), timebase));
            }
            QYReadHost(result, errors);
            QYReadDisks(result, errors);
            QYReadGPUs(result, errors);
            QYReadNetwork(result, errors);
            result[@"errors"] = errors;
            return QYJSON(result);
        } @catch (NSException *exception) {
            return QYJSON(@{@"errors": @[@"系统状态暂时不可用"], @"network_status": @"unavailable"});
        }
    }
}
