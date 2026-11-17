// Public Accessibility and NSWorkspace only. This never opens Notification
// Center, reads app storage, or asks for permission during background polling.
#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#import <Security/Security.h>
#import <os/log.h>
#import <stdatomic.h>
#include <unistd.h>

typedef bool (*QYActive)(void *);
static NSMutableDictionary<NSString *,id> *notificationActions;
static NSObject *actionsLock;
static void prepare_actions(void) {static dispatch_once_t once;dispatch_once(&once,^{actionsLock=[NSObject new];notificationActions=[NSMutableDictionary new];});}
void qy_notifications_clear(void) {prepare_actions();@synchronized(actionsLock){[notificationActions removeAllObjects];}}
static NSDictionary *process_identity(void) {
    SecCodeRef own=NULL;CFDictionaryRef information=NULL;SecRequirementRef requirement=NULL;CFStringRef text=NULL;
    OSStatus status=SecCodeCopySelf(kSecCSDefaultFlags,&own);
    if(!status)status=SecCodeCheckValidity(own,kSecCSStrictValidate,NULL);
    OSStatus signingStatus=status;
    if(!signingStatus)signingStatus=SecCodeCopySigningInformation(own,kSecCSSigningInformation,&information);
    NSDictionary *signing=(__bridge NSDictionary *)information;
    if(!signingStatus)signingStatus=SecCodeCopyDesignatedRequirement(own,kSecCSDefaultFlags,&requirement);
    if(!signingStatus)signingStatus=SecRequirementCopyString(requirement,kSecCSDefaultFlags,&text);
    NSRunningApplication *parent=[NSRunningApplication runningApplicationWithProcessIdentifier:getppid()];
    NSDictionary *result=@{@"pid":@(getpid()),@"parent_pid":@(getppid()),
        @"bundle_id":NSBundle.mainBundle.bundleIdentifier?:@"",@"parent_bundle_id":parent.bundleIdentifier?:@"",
        @"executable_path":[signing[(__bridge NSString *)kSecCodeInfoMainExecutable] path]?:NSBundle.mainBundle.executablePath?:@"",
        @"signing_identifier":signing[(__bridge NSString *)kSecCodeInfoIdentifier]?:@"",
        @"signature_status":@(status),@"identity_status":@(signingStatus),
        @"designated_requirement":(__bridge NSString *)text?:@""};
    if(text)CFRelease(text);if(requirement)CFRelease(requirement);if(information)CFRelease(information);if(own)CFRelease(own);
    return result;
}
char *qy_notifications_access_diagnostics(void) {
    @autoreleasepool {
        NSMutableDictionary *result=[process_identity() mutableCopy];
        result[@"accessibility_granted"]=@((BOOL)AXIsProcessTrusted());
        // TCC chooses its responsible process; the immediate parent alone
        // cannot identify that attribution chain through a public API.
        result[@"permission_scope"]=@"current_launch_context";
        result[@"responsible_process"]=@"not_exposed_by_public_api";
        NSData *data=[NSJSONSerialization dataWithJSONObject:result options:0 error:nil];
        return data?strdup([[NSString alloc]initWithData:data encoding:NSUTF8StringEncoding].UTF8String):NULL;
    }
}
static void log_permission(BOOL permission) {
    static os_log_t log;static dispatch_once_t once;
    dispatch_once(&once,^{log=os_log_create("com.qingyu.macos","notification-access");
        NSDictionary *identity=process_identity();
        os_log(log,"Notification sampler identity pid=%{public}d bundle=%{public}@ identifier=%{public}@ signature_status=%{public}d requirement=%{public}@",getpid(),identity[@"bundle_id"],identity[@"signing_identifier"],[identity[@"signature_status"] intValue],identity[@"designated_requirement"]);
    });
    // Only permission transitions are logged; no message or AX text is kept.
    prepare_actions();@synchronized(actionsLock){static int previous=-1;
        if(previous!=(int)permission){previous=(int)permission;os_log(log,"Notification sampler authorization pid=%{public}d accessibility_granted=%{public}d scope=current_launch_context",getpid(),(int)permission);}
    }
}
static id ax_value(AXUIElementRef element,CFStringRef attribute) {CFTypeRef value=NULL;if(AXUIElementCopyAttributeValue(element,attribute,&value)!=kAXErrorSuccess)return nil;return CFBridgingRelease(value);}
static NSString *ax_string(AXUIElementRef element,CFStringRef attribute) {id value=ax_value(element,attribute);if([value isKindOfClass:NSAttributedString.class])return [(NSAttributedString *)value string];return [value isKindOfClass:NSString.class]?value:@"";}
bool qy_notifications_session_locked(void) {
    NSDictionary *session=CFBridgingRelease(CGSessionCopyCurrentDictionary());
    if(!session||![session[(__bridge NSString *)kCGSessionOnConsoleKey] boolValue]||![session[(__bridge NSString *)kCGSessionLoginDoneKey] boolValue])return YES;
    if([NSWorkspace.sharedWorkspace.frontmostApplication.bundleIdentifier isEqualToString:@"com.apple.loginwindow"])return YES;
    // A screen-lock loginwindow shield is visible even while Workspace still
    // reports the app that was active before locking. Only window metadata is read.
    NSArray *windows=CFBridgingRelease(CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly|kCGWindowListExcludeDesktopElements,kCGNullWindowID));
    for(NSDictionary *window in windows) {
        if([window[(__bridge NSString *)kCGWindowLayer] intValue] < CGWindowLevelForKey(kCGScreenSaverWindowLevelKey))continue;
        NSRunningApplication *app=[NSRunningApplication runningApplicationWithProcessIdentifier:[window[(__bridge NSString *)kCGWindowOwnerPID] intValue]];
        if([app.bundleIdentifier isEqualToString:@"com.apple.loginwindow"])return YES;
    }
    return NO;
}
static BOOL allowed(NSString *bundle,NSDictionary *config) {
    NSString *lower=bundle.lowercaseString;
    for(NSString *blocked in config[@"disabled"])if([lower isEqualToString:blocked.lowercaseString])return NO;
    for(NSDictionary *rule in config[@"rules"]) {
        NSString *identifier=rule[@"id"];
        if(![rule[@"prefix"] boolValue]&&[lower isEqualToString:identifier])return YES;
        if([rule[@"prefix"] boolValue]&&[lower hasPrefix:identifier]) {
            NSString *suffix=[lower substringFromIndex:identifier.length];
            if(suffix.length&&[suffix rangeOfCharacterFromSet:NSCharacterSet.decimalDigitCharacterSet.invertedSet].location==NSNotFound)return YES;
        }
    }
    return NO;
}
static NSArray *children(AXUIElementRef element) {id result=ax_value(element,kAXChildrenAttribute);return [result isKindOfClass:NSArray.class]?result:@[];}
static BOOL valid(QYActive active,void *context,CFAbsoluteTime deadline) {return active(context)&&CFAbsoluteTimeGetCurrent()<deadline;}
static void dock_walk(AXUIElementRef element,NSUInteger depth,NSUInteger *budget,CFAbsoluteTime deadline,NSDictionary *config,NSMutableArray *items,QYActive active,void *context) {
    if(depth>7||!*budget||!valid(active,context,deadline))return;--*budget;
    NSString *subrole=ax_string(element,kAXSubroleAttribute);
    if([subrole isEqualToString:@"AXApplicationDockItem"]) {
        id value=ax_value(element,kAXURLAttribute);
        NSURL *url=[value isKindOfClass:NSURL.class]?value:([value isKindOfClass:NSString.class]?[NSURL URLWithString:value]:nil);
        if(!url.isFileURL)return;
        NSBundle *bundle=[NSBundle bundleWithURL:url];NSString *identifier=bundle.bundleIdentifier;
        if(!identifier||!allowed(identifier,config)||!active(context))return;
        NSString *name=[bundle objectForInfoDictionaryKey:@"CFBundleDisplayName"]?:[bundle objectForInfoDictionaryKey:@"CFBundleName"]?:url.lastPathComponent;
        // AXStatusLabel is the Dock item's exposed Accessibility status. It
        // contains unread counts/red dots, never sender or message content.
        NSString *badge=ax_string(element,CFSTR("AXStatusLabel"));
        [items addObject:@{@"id":identifier,@"name":name?:identifier,@"badge":[badge substringToIndex:MIN(badge.length,32)]}];return;
    }
    for(id child in children(element)) {
        if(CFGetTypeID((__bridge CFTypeRef)child)==AXUIElementGetTypeID())dock_walk((__bridge AXUIElementRef)child,depth+1,budget,deadline,config,items,active,context);
    }
}

// App labels only remove source headings after exact bundle attribution. Never
// authorize content reads because a sender or message mentions an app brand.
static NSDictionary *source_labels(NSDictionary *config) {
    NSMutableDictionary *labels=[NSMutableDictionary new];
    NSArray *known=@[@[@"QQ",@"com.tencent.qq"],@[@"微信",@"com.tencent.xinWeChat"],@[@"WeChat",@"com.tencent.xinWeChat"],@[@"企业微信",@"com.tencent.WeWorkMac"],@[@"WeCom",@"com.tencent.WeWorkMac"],@[@"飞书",@"com.electron.lark"],@[@"Lark",@"com.electron.lark"],@[@"钉钉",@"com.alibaba.DingTalkMac"],@[@"DingTalk",@"com.alibaba.DingTalkMac"],@[@"邮件",@"com.apple.mail"],@[@"Mail",@"com.apple.mail"],@[@"Microsoft Outlook",@"com.microsoft.Outlook"],@[@"Spark",@"com.readdle.smartemail-Mac"],@[@"网易邮箱大师",@"com.netease.mailmaster"]];
    for(NSArray *source in known)if(allowed(source[1],config))labels[source[0]]=source[1];
    for(NSRunningApplication *app in NSWorkspace.sharedWorkspace.runningApplications) {
        if(!app.bundleIdentifier||!app.localizedName||!allowed(app.bundleIdentifier,config))continue;
        // Identical labels across multi-account instances cannot be reliably
        // attributed to one instance. A live AX action still targets the notice.
        labels[app.bundleIdentifier]=app.bundleIdentifier;
        if(!labels[app.localizedName])labels[app.localizedName]=app.bundleIdentifier;
    }
    return labels;
}
// One reader/traversal is shared by live AX sampling and structural fixtures.
// Unknown wrappers are traversed using metadata only. Content is requested only
// after an exact source has been established on this node or its app stack.
@interface QYNoticeScan : NSObject
@property NSDictionary *config;
@property NSDictionary *labels;
@property NSArray<NSString *> *identifiers;
@property NSMutableArray *previews;
@property NSMutableArray *nodes;
@property NSMutableArray *captures;
@property NSMutableDictionary *stats;
@property NSMutableDictionary *errors;
@property QYActive active;
@property void *context;
@property CFAbsoluteTime deadline;
@property NSUInteger budget;
@property NSUInteger maxDepth;
@property NSUInteger reads;
@property NSUInteger cancelAfter;
@property BOOL fixture;
@property BOOL diagnostics;
@property BOOL diagnosticOnly;
@property BOOL cancelled;
@property BOOL limited;
@end
@implementation QYNoticeScan
@end
static void count(QYNoticeScan *scan,NSString *key) {scan.stats[key]=@([scan.stats[key] unsignedIntegerValue]+1);}
static BOOL scan_valid(QYNoticeScan *scan) {
    if(!scan.active(scan.context)||(scan.cancelAfter&&scan.reads>=scan.cancelAfter)){scan.cancelled=YES;return NO;}
    if(CFAbsoluteTimeGetCurrent()>=scan.deadline){scan.limited=YES;return NO;}
    return YES;
}
static NSString *bounded(NSString *value,NSUInteger limit) {
    if(value.length<=limit)return value;
    // Do not split a UTF-16 surrogate pair at the JSON boundary.
    if(limit&&CFStringIsSurrogateHighCharacter([value characterAtIndex:limit-1]))--limit;
    return [value substringToIndex:limit];
}
static id notice_value(id node,CFStringRef attribute,QYNoticeScan *scan) {
    if(!scan_valid(scan))return nil;
    ++scan.reads;
    if(scan.fixture) {
        static NSDictionary *keys;static dispatch_once_t once;
        dispatch_once(&once,^{keys=@{@"AXRole":@"role",@"AXSubrole":@"subrole",@"AXStackingIdentifier":@"stacking",@"AXIdentifier":@"identifier",@"AXTitle":@"title",@"AXDescription":@"description",@"AXAttributedDescription":@"attributed",@"AXValue":@"value",@"AXChildren":@"children"};});
        id value=node[keys[(__bridge NSString *)attribute]?:@""];
        return value==NSNull.null?nil:value;
    }
    CFTypeRef value=NULL;AXError error=AXUIElementCopyAttributeValue((__bridge AXUIElementRef)node,attribute,&value);
    if(error!=kAXErrorSuccess){NSString *key=[NSString stringWithFormat:@"%d",error];scan.errors[key]=@([scan.errors[key] unsignedIntegerValue]+1);if(value)CFRelease(value);return nil;}
    return CFBridgingRelease(value);
}
static NSString *notice_string(id node,CFStringRef attribute,QYNoticeScan *scan) {
    id value=notice_value(node,attribute,scan);
    if([value isKindOfClass:NSAttributedString.class])return bounded([(NSAttributedString *)value string],1600);
    return [value isKindOfClass:NSString.class]?bounded(value,1600):@"";
}
static NSArray *notice_children(id node,QYNoticeScan *scan) {
    id value=notice_value(node,kAXChildrenAttribute,scan);
    if(![value isKindOfClass:NSArray.class])return @[];
    NSMutableArray *result=[NSMutableArray new];
    for(id child in value) {
        if((scan.fixture&&[child isKindOfClass:NSDictionary.class])||(!scan.fixture&&CFGetTypeID((__bridge CFTypeRef)child)==AXUIElementGetTypeID()))[result addObject:child];
        if(result.count>=128){scan.limited=YES;break;}
    }
    return result;
}
static NSString *stacking_bundle(NSString *stacking,NSArray<NSString *> *identifiers) {
    NSString *value=stacking.lowercaseString;
    for(NSString *candidate in identifiers) {
        NSString *lower=candidate.lowercaseString;
        if(![value hasSuffix:lower])continue;
        NSUInteger prefix=value.length-lower.length;
        if(prefix&&[[NSCharacterSet characterSetWithCharactersInString:@"abcdefghijklmnopqrstuvwxyz0123456789._-"] characterIsMember:[value characterAtIndex:prefix-1]])continue;
        return candidate;
    }
    return nil;
}
static BOOL explicit_other_source(NSString *stacking,NSString *identifier) {
    // An unrecognized wrapper must not stop discovery below it. A different
    // app's explicit source, however, must not inherit a selected app's source.
    if([stacking.lowercaseString hasPrefix:@"notification-app:"])return YES;
    NSString *value=stacking.length?stacking:identifier;
    if([value.lowercaseString containsString:@"com.apple.notificationcenter"])return NO;
    return [value rangeOfString:@"(^|[: /])([a-zA-Z0-9_-]+\\.){2,}[a-zA-Z0-9_.-]+$" options:NSRegularExpressionSearch].location!=NSNotFound;
}
static NSString *safe_role(NSString *value) {
    return value.length<=64&&[value rangeOfString:@"^AX[A-Za-z]+$" options:NSRegularExpressionSearch].location!=NSNotFound?value:@"other";
}
static NSString *source_name(NSString *bundle,QYNoticeScan *scan) {
    if(!scan.fixture)for(NSRunningApplication *app in [NSRunningApplication runningApplicationsWithBundleIdentifier:bundle])if(app.localizedName.length)return app.localizedName;
    for(NSString *name in [[scan.labels allKeys] sortedArrayUsingSelector:@selector(compare:)])if([scan.labels[name] caseInsensitiveCompare:bundle]==NSOrderedSame&&![name.lowercaseString isEqualToString:bundle.lowercaseString])return name;
    return bundle;
}
static BOOL source_text(NSString *text,NSString *source,NSString *bundle,QYNoticeScan *scan) {
    NSString *value=[text stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
    if(!value.length||[value isEqualToString:source]||[value caseInsensitiveCompare:bundle]==NSOrderedSame)return YES;
    NSString *match=scan.labels[value];
    if(match&&[match caseInsensitiveCompare:bundle]==NSOrderedSame)return YES;
    if(([bundle.lowercaseString hasPrefix:@"com.wechatmulti.clone"]||[@[@"com.tencent.xinwechat",@"com.tencent.wechat"] containsObject:bundle.lowercaseString])&&[@[@"微信",@"wechat"] containsObject:value.lowercaseString])return YES;
    return NO;
}
static BOOL time_text(NSString *text) {
    NSString *value=[text.lowercaseString stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
    return [@[@"now",@"现在",@"刚刚",@"today",@"yesterday",@"今天",@"昨天"] containsObject:value]
        ||[value rangeOfString:@"^([0-9]+\\s*(秒钟?|分钟?|小时|天|周)前|[0-9]+\\s*(s|m|h|d|sec(ond)?s?|min(ute)?s?|hours?|days?)\\s*(ago)?|[0-9]{1,2}:[0-9]{2}(\\s*[ap]m)?)$" options:NSRegularExpressionSearch].location!=NSNotFound;
}
static BOOL usable_aggregate(NSString *value,NSString *source,NSString *bundle,QYNoticeScan *scan) {
    if(!value.length||source_text(value,source,bundle,scan)||time_text(value))return NO;
    if([@[@"notification",@"notifications",@"通知",@"通知中心",@"notification center",@"alert",@"close",@"dismiss",@"clear",@"clear all",@"show more",@"show less",@"options",@"reply",@"关闭",@"关闭通知",@"清除通知",@"close notification",@"dismiss notification",@"清除",@"全部清除",@"显示更多",@"显示更少",@"选项",@"回复"] containsObject:value.lowercaseString])return NO;
    // This only rejects descriptions consisting entirely of source/time labels;
    // it never assigns sender/body by splitting an aggregate on punctuation.
    NSArray *parts=[value componentsSeparatedByCharactersInSet:[NSCharacterSet characterSetWithCharactersInString:@",，\n"]];
    for(NSString *part in parts)if(!source_text(part,source,bundle,scan)&&!time_text(part)&&![@[@"notification",@"通知"] containsObject:part.lowercaseString])return YES;
    return NO;
}
static BOOL dedicated_notification_shape(NSString *role,NSString *subrole,NSString *identifier) {
    NSString *lower=subrole.lowercaseString;
    if([lower containsString:@"close"]||[identifier.lowercaseString containsString:@"dismiss"]||[identifier.lowercaseString containsString:@"close"])return NO;
    if([lower containsString:@"stack"]||[lower containsString:@"list"]||[lower isEqualToString:@"axnotificationcenter"])return NO;
    return [lower containsString:@"notification"]
        ||([role isEqualToString:(__bridge NSString *)kAXGroupRole]&&[identifier.lowercaseString hasPrefix:@"notification-"]);
}
static BOOL notification_shape(NSString *role,NSString *subrole,NSString *identifier) {
    if(dedicated_notification_shape(role,subrole,identifier))return YES;
    NSString *lower=subrole.lowercaseString;
    return [role isEqualToString:(__bridge NSString *)kAXButtonRole]&&![lower containsString:@"close"]
        &&![lower containsString:@"stack"]&&![lower containsString:@"list"]&&![lower isEqualToString:@"axnotificationcenter"]
        &&![identifier.lowercaseString containsString:@"dismiss"]&&![identifier.lowercaseString containsString:@"close"];
}
static BOOL notice_control(NSString *role) {
    return [@[@"AXButton",@"AXMenuButton",@"AXPopUpButton",@"AXCheckBox",@"AXRadioButton",@"AXTextField",@"AXTextArea"] containsObject:role];
}
static void collect_notice_text(id node,NSUInteger depth,NSString *bundle,NSMutableArray<NSString *> *texts,QYNoticeScan *scan) {
    if(depth>8||texts.count>=24||!scan_valid(scan))return;
    if(!scan.budget){scan.limited=YES;return;}--scan.budget;
    NSString *stacking=notice_string(node,CFSTR("AXStackingIdentifier"),scan),*identifier=notice_string(node,CFSTR("AXIdentifier"),scan);
    NSString *own=stacking_bundle(stacking,scan.identifiers)?:stacking_bundle(identifier,scan.identifiers);
    if((own&&[own caseInsensitiveCompare:bundle]!=NSOrderedSame)||(!own&&explicit_other_source(stacking,identifier)))return;
    NSString *role=notice_string(node,kAXRoleAttribute,scan);
    // Reply, expand and other controls may expose their own static labels.
    // They are not notification content even when their wording is unknown.
    if(depth&&notice_control(role))return;
    if([role isEqualToString:(__bridge NSString *)kAXStaticTextRole]) {
        count(scan,@"content_nodes");
        NSString *value=notice_string(node,kAXValueAttribute,scan);
        if(!value.length)value=notice_string(node,kAXTitleAttribute,scan);
        if(!value.length)value=notice_string(node,kAXDescriptionAttribute,scan);
        if(!value.length)value=notice_string(node,CFSTR("AXAttributedDescription"),scan);
        if(value.length&&![texts.lastObject isEqualToString:value])[texts addObject:bounded(value,1200)];
    }
    for(id child in notice_children(node,scan))collect_notice_text(child,depth+1,bundle,texts,scan);
}
static NSDictionary *preview_fields(NSArray<NSString *> *texts,NSString *source,NSString *bundle,NSString *title,NSString *description,NSString *attributed,NSString *value,QYNoticeScan *scan) {
    NSMutableArray<NSString *> *content=[NSMutableArray new];
    for(NSString *text in texts) {
        if(!content.count&&(source_text(text,source,bundle,scan)||time_text(text)))continue;
        [content addObject:text];
    }
    if(content.count>1&&time_text(content.lastObject))[content removeLastObject];
    BOOL hasTitle=usable_aggregate(title,source,bundle,scan);
    NSString *sender=@"",*body=@"",*path=@"none";
    if(content.count>=2) {
        sender=content.firstObject;body=[[content subarrayWithRange:NSMakeRange(1,content.count-1)] componentsJoinedByString:@"\n"];path=@"static-text";
    } else {
        if(content.count&&hasTitle&&[content.firstObject isEqualToString:title])sender=title;
        else if(hasTitle)sender=title;
        if(usable_aggregate(description,source,bundle,scan)){body=description;path=@"description";}
        else if(usable_aggregate(attributed,source,bundle,scan)){body=attributed;path=@"attributed-description";}
        else if(usable_aggregate(value,source,bundle,scan)){body=value;path=@"value";}
        else if(content.count) {
            if(!sender.length){body=content.firstObject;path=@"static-text";}
            else if(![sender isEqualToString:content.firstObject]){body=content.firstObject;path=@"static-text";}
        }
        if(!body.length&&sender.length)path=@"title-only";
    }
    return @{@"sender":bounded(sender,320),@"body":bounded(body,1200),@"path":path};
}
static NSString *notice_identity(pid_t pid,NSUInteger element,NSString *sender,NSString *body) {
    // Native node + content distinguish equal simultaneous notices and reused
    // nodes. Rust recognizes monotonic hydration and keeps its delivery key.
    return [NSString stringWithFormat:@"ax:%d:%lu:%lu:%lu",pid,(unsigned long)element,(unsigned long)sender.hash,(unsigned long)body.hash];
}
static BOOL notice_pressable(id node,QYNoticeScan *scan) {
    if(!scan_valid(scan))return NO;
    if(scan.fixture)return [node[@"actions"] containsObject:(__bridge NSString *)kAXPressAction];
    CFArrayRef actions=NULL;BOOL pressable=NO;
    if(AXUIElementCopyActionNames((__bridge AXUIElementRef)node,&actions)==kAXErrorSuccess&&actions)
        pressable=[(__bridge NSArray *)actions containsObject:(__bridge NSString *)kAXPressAction];
    if(actions)CFRelease(actions);return pressable;
}
static BOOL notice_walk(id node,pid_t pid,NSUInteger depth,NSString *inherited,BOOL inNotice,QYNoticeScan *scan) {
    if(!scan_valid(scan))return NO;
    if(depth>scan.maxDepth||!scan.budget){scan.limited=YES;return NO;}
    --scan.budget;count(scan,@"visited");
    NSString *role=notice_string(node,kAXRoleAttribute,scan);
    if([role isEqualToString:@"AXMenuBar"]||[role isEqualToString:@"AXMenu"]||[role isEqualToString:@"AXMenuItem"]){count(scan,@"skipped_menus");return NO;}
    NSString *subrole=notice_string(node,kAXSubroleAttribute,scan);
    NSString *stacking=notice_string(node,CFSTR("AXStackingIdentifier"),scan),*identifier=notice_string(node,CFSTR("AXIdentifier"),scan);
    NSString *bundle=stacking_bundle(stacking,scan.identifiers),*mode=bundle?@"stacking":@"none";
    if(!bundle){bundle=stacking_bundle(identifier,scan.identifiers);if(bundle)mode=@"identifier";}
    BOOL other=!bundle&&explicit_other_source(stacking,identifier),shape=notification_shape(role,subrole,identifier);
    if(!bundle&&!other&&inherited){bundle=inherited;mode=@"inherited";}
    // Source inheritance identifies both notice content and its controls.
    // A generic button inside an identified notice cannot establish another
    // message. Separate buttons directly under an app stack remain notices.
    if(inNotice&&notice_control(role)&&!dedicated_notification_shape(role,subrole,identifier))shape=NO;
    // A sender/title equal to an app name is not source metadata. Only an
    // exact public bundle identifier (possibly on the enclosing stack) can
    // authorize reading notification content.
    if(bundle)count(scan,@"attributed_nodes");
    else if(stacking.length||identifier.length)count(scan,@"unknown_source_nodes");
    NSArray *list=notice_children(node,scan);
    NSMutableDictionary *trace=nil;
    if(scan.diagnostics&&scan.nodes.count<128) {
        trace=[@{@"depth":@(depth),@"role":safe_role(role),@"subrole":safe_role(subrole),@"children":@(list.count),@"stacking":stacking.length?(stacking_bundle(stacking,scan.identifiers)?@"exact":@"unknown"):@"none",@"identifier":identifier.length?(stacking_bundle(identifier,scan.identifiers)?@"exact":@"unknown"):@"none",@"source_mode":mode,@"candidate":@(shape)} mutableCopy];
        if(bundle)trace[@"bundle_id"]=bundle;
        [scan.nodes addObject:trace];
    }
    // Parse each leaf notification before its enclosing stack. Never fabricate
    // one combined message out of a stack containing several native notices.
    BOOL found=NO;
    for(id child in list)found=notice_walk(child,pid,depth+1,bundle,inNotice||(bundle&&shape),scan)||found;
    if(found||!bundle||!shape||!scan_valid(scan))return found;
    count(scan,@"candidates");
    NSMutableArray *texts=[NSMutableArray new];collect_notice_text(node,0,bundle,texts,scan);
    if(!scan_valid(scan))return NO;
    NSString *source=source_name(bundle,scan),*title=notice_string(node,kAXTitleAttribute,scan);
    // App/time-only child text must not prevent aggregate fallback on Sequoia.
    NSString *description=notice_string(node,kAXDescriptionAttribute,scan),*attributed=notice_string(node,CFSTR("AXAttributedDescription"),scan),*value=notice_string(node,kAXValueAttribute,scan);
    NSDictionary *fields=preview_fields(texts,source,bundle,title,description,attributed,value,scan);
    NSString *sender=fields[@"sender"],*body=fields[@"body"];
    if(trace){trace[@"text_fields"]=@(texts.count);trace[@"title_length"]=@(title.length);trace[@"description_length"]=@(description.length);trace[@"attributed_length"]=@(attributed.length);trace[@"parser"]=fields[@"path"];trace[@"sender_length"]=@(sender.length);trace[@"body_length"]=@(body.length);}
    if(!sender.length&&!body.length){count(scan,@"empty_candidates");return NO;}
    if(!scan_valid(scan))return NO;
    NSUInteger element=scan.fixture?[node[@"element"] unsignedIntegerValue]:(NSUInteger)CFHash((__bridge AXUIElementRef)node);
    NSString *key=notice_identity(pid,element,sender,body),*activation=@"";
    if(!scan.diagnosticOnly&&notice_pressable(node,scan)) {
        if(scan.fixture)activation=key;
        else {prepare_actions();@synchronized(actionsLock){if(scan_valid(scan)){if(notificationActions.count>=128)[notificationActions removeAllObjects];notificationActions[key]=node;activation=key;}}}
    }
    if(!scan_valid(scan))return NO;
    [scan.previews addObject:@{@"id":key,@"bundle_id":bundle,@"name":source,@"sender":sender,@"body":body,@"activation":activation}];
    [scan.captures addObject:@{@"bundle_id":bundle,@"source_mode":mode,@"parser":fields[@"path"],@"sender_length":@(sender.length),@"body_length":@(body.length)}];
    count(scan,@"captured");return YES;
}
static QYNoticeScan *new_scan(NSDictionary *config,NSDictionary *labels,QYActive active,void *context) {
    QYNoticeScan *scan=[QYNoticeScan new];scan.config=config;scan.labels=labels;
    NSMutableSet *identifiers=[NSMutableSet setWithArray:labels.allValues];
    for(NSDictionary *rule in config[@"rules"])if(![rule[@"prefix"] boolValue]&&allowed(rule[@"id"],config))[identifiers addObject:rule[@"id"]];
    scan.identifiers=[[identifiers allObjects] sortedArrayUsingComparator:^NSComparisonResult(NSString *a,NSString *b){return a.length>b.length?NSOrderedAscending:(a.length<b.length?NSOrderedDescending:[a compare:b]);}];
    scan.previews=[NSMutableArray new];scan.nodes=[NSMutableArray new];scan.captures=[NSMutableArray new];scan.stats=[NSMutableDictionary new];scan.errors=[NSMutableDictionary new];
    scan.active=active;scan.context=context;scan.deadline=CFAbsoluteTimeGetCurrent()+0.9;scan.budget=480;scan.maxDepth=20;scan.diagnostics=[config[@"diagnostics"] boolValue];scan.diagnosticOnly=[config[@"diagnostic_only"] boolValue];return scan;
}
static NSDictionary *scan_diagnostics(QYNoticeScan *scan) {
    return @{@"statistics":scan.stats,@"attribute_errors":scan.errors,@"nodes":scan.nodes,@"captures":scan.captures,@"attribute_reads":@(scan.reads),@"bounded":@(scan.limited),@"cancelled":@(scan.cancelled)};
}
static BOOL fixture_active(void *context){return YES;}
// Full-tree fixtures use exactly the live source gate, traversal and parser.
// Test-only input/output can contain synthetic content; diagnostics never do.
char *qy_notifications_fixture(const char *input) {
    @autoreleasepool {
        NSDictionary *fixture=[NSJSONSerialization JSONObjectWithData:[NSData dataWithBytes:input length:strlen(input)] options:0 error:nil];
        NSDictionary *config=fixture[@"config"]?:@{};NSMutableDictionary *labels=[source_labels(config) mutableCopy];
        [labels addEntriesFromDictionary:fixture[@"labels"]?:@{}];
        QYNoticeScan *scan=new_scan(config,labels,fixture_active,NULL);scan.fixture=YES;scan.diagnostics=YES;
        NSMutableSet *identifiers=[NSMutableSet setWithArray:scan.identifiers];for(NSString *identifier in fixture[@"identifiers"])if(allowed(identifier,config))[identifiers addObject:identifier];scan.identifiers=identifiers.allObjects;
        scan.cancelAfter=[fixture[@"cancel_after"] unsignedIntegerValue];if(fixture[@"budget"])scan.budget=MIN([fixture[@"budget"] unsignedIntegerValue],480);
        id tree=fixture[@"tree"];
        if(!tree) {
            NSMutableArray *children=[NSMutableArray new];for(NSString *value in fixture[@"texts"])[children addObject:@{@"role":@"AXStaticText",@"value":value}];
            tree=@{@"role":@"AXButton",@"stacking":fixture[@"stacking"]?:@"",@"title":fixture[@"title"]?:@"",@"description":fixture[@"description"]?:@"",@"attributed":fixture[@"attributed"]?:@"",@"children":children,@"element":fixture[@"element"]?:@0};
            if([fixture[@"source"] length]){NSMutableDictionary *named=[scan.labels mutableCopy];named[fixture[@"source"]]=stacking_bundle(fixture[@"stacking"]?:@"",scan.identifiers)?:@"";scan.labels=named;}
        }
        notice_walk(tree,[fixture[@"pid"] intValue],0,nil,NO,scan);
        if(scan.cancelled){[scan.previews removeAllObjects];[scan.captures removeAllObjects];}
        NSDictionary *result=fixture[@"tree"]?@{@"previews":scan.previews,@"diagnostics":scan_diagnostics(scan)}:(scan.previews.firstObject?:@{});
        NSData *data=[NSJSONSerialization dataWithJSONObject:result options:0 error:nil];return data?strdup([[NSString alloc]initWithData:data encoding:NSUTF8StringEncoding].UTF8String):NULL;
    }
}
char *qy_notifications_sample(const char *rules,QYActive active,void *context) {
    @autoreleasepool {
        NSDictionary *config=[NSJSONSerialization JSONObjectWithData:[NSData dataWithBytes:rules length:strlen(rules)] options:0 error:nil];
        BOOL locked=qy_notifications_session_locked(),permission=AXIsProcessTrusted(),ready=NO,previewAccessible=NO;
        log_permission(permission);
        NSMutableArray *items=[NSMutableArray new];QYNoticeScan *scan=new_scan(config,source_labels(config),active,context);
        NSMutableDictionary *rootStatus=[NSMutableDictionary new];
        if(!locked&&permission&&active(context)) {
            NSRunningApplication *dock=[NSRunningApplication runningApplicationsWithBundleIdentifier:@"com.apple.dock"].firstObject;
            if(dock) {
                AXUIElementRef root=AXUIElementCreateApplication(dock.processIdentifier);AXUIElementSetMessagingTimeout(root,0.08);
                CFTypeRef list=NULL;AXError error=AXUIElementCopyAttributeValue(root,kAXChildrenAttribute,&list);ready=error==kAXErrorSuccess;
                rootStatus[@"dock_children_error"]=@(error);if(list)CFRelease(list);
                NSUInteger budget=160;dock_walk(root,0,&budget,CFAbsoluteTimeGetCurrent()+0.4,config,items,active,context);CFRelease(root);
            }
            if([config[@"previews"] boolValue]&&active(context)) {
                NSRunningApplication *center=[NSRunningApplication runningApplicationsWithBundleIdentifier:@"com.apple.notificationcenterui"].firstObject;
                rootStatus[@"notification_center_running"]=@(center!=nil);
                if(center) {
                    AXUIElementRef root=AXUIElementCreateApplication(center.processIdentifier);AXUIElementSetMessagingTimeout(root,0.08);
                    CFTypeRef windows=NULL;AXError error=AXUIElementCopyAttributeValue(root,kAXWindowsAttribute,&windows);
                    rootStatus[@"windows_error"]=@(error);
                    scan.deadline=CFAbsoluteTimeGetCurrent()+0.9;
                    if(error==kAXErrorSuccess) {
                        previewAccessible=YES;
                        if(windows&&CFGetTypeID(windows)==CFArrayGetTypeID()) {
                            rootStatus[@"window_count"]=@([(__bridge NSArray *)windows count]);
                            for(id window in (__bridge NSArray *)windows)if(CFGetTypeID((__bridge CFTypeRef)window)==AXUIElementGetTypeID())notice_walk(window,center.processIdentifier,0,nil,NO,scan);
                        }
                    }
                    if(windows)CFRelease(windows);
                    // Some releases expose banners under AXChildren while the
                    // AXWindows list is empty. Inspect that public root too.
                    if(!scan.previews.count&&scan_valid(scan)) {
                        id rawChildren=notice_value((__bridge id)root,kAXChildrenAttribute,scan);
                        NSArray *list=[rawChildren isKindOfClass:NSArray.class]?rawChildren:@[];
                        previewAccessible=previewAccessible||[rawChildren isKindOfClass:NSArray.class];
                        if(list.count>128){list=[list subarrayWithRange:NSMakeRange(0,128)];scan.limited=YES;}
                        rootStatus[@"root_children"]=@(list.count);
                        for(id child in list)if(CFGetTypeID((__bridge CFTypeRef)child)==AXUIElementGetTypeID())notice_walk(child,center.processIdentifier,0,nil,NO,scan);
                    }
                    CFRelease(root);
                }
            }
        }
        locked=locked||qy_notifications_session_locked();
        // A grant can be revoked while an AX walk is in flight. Do not publish
        // its content under an earlier permission result.
        if(permission){permission=AXIsProcessTrusted();log_permission(permission);}
        if(locked||!permission||!active(context)||scan.cancelled){[items removeAllObjects];[scan.previews removeAllObjects];[scan.captures removeAllObjects];qy_notifications_clear();}
        BOOL previewReady=NO;for(NSDictionary *preview in scan.previews)if([preview[@"body"] length]){previewReady=YES;break;}
        NSMutableDictionary *result=[@{@"permission":@(permission),@"locked":@(locked),@"ready":@(ready),@"preview_accessible":@(previewAccessible),@"preview_ready":@(previewReady),@"items":items,@"previews":scan.diagnosticOnly?@[]:scan.previews} mutableCopy];
        if(scan.diagnostics){NSMutableDictionary *diagnostics=[scan_diagnostics(scan) mutableCopy];diagnostics[@"roots"]=rootStatus;result[@"diagnostics"]=diagnostics;}
        NSData *data=[NSJSONSerialization dataWithJSONObject:result options:0 error:nil];return data?strdup([[NSString alloc]initWithData:data encoding:NSUTF8StringEncoding].UTF8String):NULL;
    }
}
void qy_notifications_free(char *value) {if(value){volatile char *cursor=value;size_t remaining=strlen(value);while(remaining--)*cursor++=0;free(value);}}
bool qy_notifications_access_granted(void) {return AXIsProcessTrusted();}
void qy_notifications_request_access(void) {
    NSDictionary *options=@{(__bridge NSString *)kAXTrustedCheckOptionPrompt:@YES};AXIsProcessTrustedWithOptions((__bridge CFDictionaryRef)options);
    [NSWorkspace.sharedWorkspace openURL:[NSURL URLWithString:@"x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"]];
}
static NSURL *notification_application_url(NSString *identifier) {
    // Prefer the exact running instance; Launch Services can resolve a second
    // installed copy with the same bundle ID to a different WeChat version.
    for(NSRunningApplication *app in [NSRunningApplication runningApplicationsWithBundleIdentifier:identifier])if(app.bundleURL)return app.bundleURL;
    NSString *standard=nil;
    if([identifier.lowercaseString isEqualToString:@"com.tencent.xinwechat"])standard=@"/Applications/WeChat.app";
    else if([identifier.lowercaseString isEqualToString:@"com.tencent.qq"])standard=@"/Applications/QQ.app";
    if(standard) {
        NSURL *url=[NSURL fileURLWithPath:standard];NSString *bundle=[NSBundle bundleWithURL:url].bundleIdentifier;
        if(bundle.length&&[bundle caseInsensitiveCompare:identifier]==NSOrderedSame)return url;
    }
    return [NSWorkspace.sharedWorkspace URLForApplicationWithBundleIdentifier:identifier];
}
static BOOL activate_notification_application(NSString *identifier) {
    for(NSRunningApplication *app in [NSRunningApplication runningApplicationsWithBundleIdentifier:identifier])if([app activateWithOptions:NSApplicationActivateIgnoringOtherApps])return YES;
    NSURL *url=notification_application_url(identifier);if(!url)return NO;
    [NSWorkspace.sharedWorkspace openApplicationAtURL:url configuration:NSWorkspaceOpenConfiguration.configuration completionHandler:nil];return YES;
}
bool qy_notifications_open(const char *bundle,const char *activation) {
    @autoreleasepool {
        if(qy_notifications_session_locked())return false;
        NSString *identifier=[NSString stringWithUTF8String:bundle],*key=[NSString stringWithUTF8String:activation];
        prepare_actions();id element=nil;@synchronized(actionsLock){element=notificationActions[key];}
        if(element) {
            // AXPress is a public action on the original notification; it may
            // open the precise conversation if the source app supports it.
            dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED,0),^{
                AXUIElementRef target=(__bridge AXUIElementRef)element;AXUIElementSetMessagingTimeout(target,0.2);
                if(!qy_notifications_session_locked()&&AXUIElementPerformAction(target,kAXPressAction)==kAXErrorSuccess)return;
                dispatch_async(dispatch_get_main_queue(),^{activate_notification_application(identifier);});
            });return true;
        }
        return activate_notification_application(identifier);
    }
}
