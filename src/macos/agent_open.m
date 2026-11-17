// Existing-window activation only. No CLI, conversation contents, argv, or
// environment is read. Accessibility/Automation are used only if already granted.
#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#include <libproc.h>
#include <sys/stat.h>
#include <unistd.h>

static BOOL qy_process(pid_t pid,struct proc_bsdinfo *info) {
    return pid>1 && proc_pidinfo(pid,PROC_PIDTBSDINFO,0,info,sizeof(*info))==sizeof(*info) && info->pbi_uid==getuid();
}
static uint64_t qy_stamp(struct proc_bsdinfo info) {return info.pbi_start_tvsec*1000000+info.pbi_start_tvusec;}
static NSString *qy_executable(pid_t pid) {
    char path[PROC_PIDPATHINFO_MAXSIZE]={0};
    return proc_pidpath(pid,path,sizeof(path))>0?[NSString stringWithUTF8String:path]:@"";
}
static NSDictionary *qy_app(NSRunningApplication *app) {
    struct proc_bsdinfo info={0};if(!app.bundleIdentifier||!app.bundleURL||!qy_process(app.processIdentifier,&info))return nil;
    return @{ @"pid":@(app.processIdentifier),@"start_stamp":@(qy_stamp(info)),@"bundle_id":app.bundleIdentifier,@"app_path":app.bundleURL.path };
}
static char *qy_json(id value) {
    NSData *data=[NSJSONSerialization dataWithJSONObject:value options:0 error:nil];
    return data?strdup([[NSString alloc]initWithData:data encoding:NSUTF8StringEncoding].UTF8String):NULL;
}
char *qy_agent_open_inspect(uint32_t pid) {
    @autoreleasepool {
        NSMutableArray *chain=[NSMutableArray new],*running=[NSMutableArray new],*installed=[NSMutableArray new];
        NSMutableSet *seen=[NSMutableSet new];
        for(int depth=0;pid>1&&depth<24;depth++) {
            if([seen containsObject:@(pid)])break;[seen addObject:@(pid)];
            struct proc_bsdinfo info={0};if(!qy_process(pid,&info))break;
            NSRunningApplication *app=[NSRunningApplication runningApplicationWithProcessIdentifier:pid];
            NSString *tty=@"";if(info.e_tdev!=NODEV) {char *name=devname(info.e_tdev,S_IFCHR);if(name)tty=[@"/dev/" stringByAppendingString:[NSString stringWithUTF8String:name]];}
            NSString *name=[[NSString alloc]initWithBytes:info.pbi_name length:strnlen(info.pbi_name,sizeof(info.pbi_name)) encoding:NSUTF8StringEncoding]?:@"";
            [chain addObject:@{@"pid":@(pid),@"parent":@(info.pbi_ppid),@"start_stamp":@(qy_stamp(info)),@"executable":qy_executable(pid)?:@"",
                @"name":name,@"bundle_id":app.bundleIdentifier?:@"",@"app_path":app.bundleURL.path?:@"",@"regular":(app&&app.activationPolicy==NSApplicationActivationPolicyRegular?@YES:@NO),@"tty":tty}];
            pid=info.pbi_ppid;
        }
        NSSet *bundles=[NSSet setWithArray:@[@"com.openai.codex",@"com.anthropic.claudefordesktop"]];
        for(NSRunningApplication *app in NSWorkspace.sharedWorkspace.runningApplications) {
            if(![bundles containsObject:app.bundleIdentifier]||app.terminated)continue;NSDictionary *value=qy_app(app);if(value)[running addObject:value];
        }
        for(NSString *bundle in bundles) {
            NSURL *url=[NSWorkspace.sharedWorkspace URLForApplicationWithBundleIdentifier:bundle];
            if(url&&[[NSBundle bundleWithURL:url].bundleIdentifier isEqualToString:bundle])
                [installed addObject:@{@"pid":@0,@"start_stamp":@0,@"bundle_id":bundle,@"app_path":url.path}];
        }
        return qy_json(@{@"chain":chain,@"running":running,@"installed":installed});
    }
}
void qy_agent_open_free(char *value) {free(value);}

static BOOL qy_same_process(NSDictionary *value) {
    struct proc_bsdinfo info={0};pid_t pid=[value[@"pid"] intValue];
    return qy_process(pid,&info)&&qy_stamp(info)==[value[@"start_stamp"] unsignedLongLongValue]
        &&[qy_executable(pid) isEqualToString:value[@"executable"]];
}
static BOOL qy_still_ancestor(pid_t source,pid_t target) {
    for(int depth=0;source>1&&depth<24;depth++) {
        if(source==target)return YES;struct proc_bsdinfo info={0};if(!qy_process(source,&info)||info.pbi_ppid==source)return NO;source=info.pbi_ppid;
    }return NO;
}
static NSRunningApplication *qy_running(NSDictionary *target) {
    NSRunningApplication *app=[NSRunningApplication runningApplicationWithProcessIdentifier:[target[@"pid"] intValue]];
    struct proc_bsdinfo info={0};
    if(app.terminated||!qy_process(app.processIdentifier,&info)||qy_stamp(info)!=[target[@"start_stamp"] unsignedLongLongValue]
        ||![app.bundleIdentifier isEqualToString:target[@"bundle_id"]]||![app.bundleURL.path isEqualToString:target[@"app_path"]])return nil;
    return app;
}
static BOOL qy_terminal_tab(NSString *tty) {
    if(![tty hasPrefix:@"/dev/tty"]||[tty rangeOfCharacterFromSet:[[NSCharacterSet characterSetWithCharactersInString:@"/abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"] invertedSet]].location!=NSNotFound)return NO;
    const char *bundle="com.apple.Terminal";AEAddressDesc target={0};
    if(AECreateDesc(typeApplicationBundleID,bundle,strlen(bundle),&target)!=noErr)return NO;
    OSStatus allowed=AEDeterminePermissionToAutomateTarget(&target,typeWildCard,typeWildCard,false);AEDisposeDesc(&target);
    if(allowed!=noErr)return NO; // Never trigger an Automation consent dialog.
    NSString *source=[NSString stringWithFormat:@"with timeout of 1 seconds\ntell application id \"com.apple.Terminal\"\nrepeat with w in windows\nrepeat with t in tabs of w\nif tty of t is equal to \"%@\" then\nset selected tab of w to t\nset index of w to 1\nreturn true\nend if\nend repeat\nend repeat\nend tell\nend timeout\nreturn false",tty];
    NSDictionary *error=nil;NSAppleEventDescriptor *result=[[[NSAppleScript alloc]initWithSource:source]executeAndReturnError:&error];
    return !error&&result.booleanValue;
}
static void qy_raise_existing_window(pid_t pid,NSString *idValue) {
    if(!AXIsProcessTrusted()||idValue.length<8||idValue.length>160)return;
    AXUIElementRef app=AXUIElementCreateApplication(pid);AXUIElementSetMessagingTimeout(app,0.15);
    CFTypeRef copied=NULL;if(AXUIElementCopyAttributeValue(app,kAXWindowsAttribute,&copied)!=kAXErrorSuccess){CFRelease(app);return;}
    NSArray *windows=CFBridgingRelease(copied);NSMutableArray *matches=[NSMutableArray new];
    for(id window in [windows subarrayWithRange:NSMakeRange(0,MIN(windows.count,16))]) {
        CFTypeRef title=NULL;if(AXUIElementCopyAttributeValue((__bridge AXUIElementRef)window,kAXTitleAttribute,&title)==kAXErrorSuccess) {
            NSString *text=CFBridgingRelease(title);if([text isKindOfClass:NSString.class]&&[text rangeOfString:idValue options:NSCaseInsensitiveSearch].location!=NSNotFound)[matches addObject:window];
        }
    }
    if(matches.count==1)AXUIElementPerformAction((__bridge AXUIElementRef)matches.firstObject,kAXRaiseAction);CFRelease(app);
}

@interface QYAgentOpenResult : NSObject
@property(strong) dispatch_semaphore_t done;
@property(strong) NSString *error;
@property BOOL finished;
@end
@implementation QYAgentOpenResult
@end
static void qy_finish(QYAgentOpenResult *result,NSString *error) {
    @synchronized(result){if(result.finished)return;result.finished=YES;result.error=error?:@"";dispatch_semaphore_signal(result.done);}
}
static BOOL qy_pending(QYAgentOpenResult *result) {@synchronized(result){return !result.finished;}}
static void qy_open_application(NSURL *url,QYAgentOpenResult *result) {
    NSWorkspaceOpenConfiguration *configuration=NSWorkspaceOpenConfiguration.configuration;configuration.activates=YES;configuration.createsNewApplicationInstance=NO;
    [NSWorkspace.sharedWorkspace openApplicationAtURL:url configuration:configuration completionHandler:^(NSRunningApplication *app,NSError *error){qy_finish(result,error?@"无法打开对应的 Agent 应用":nil);}];
}
char *qy_agent_open_perform(const char *raw) {
    @autoreleasepool {
        if(NSThread.isMainThread)return strdup("请从后台处理应用窗口定位");
        NSData *data=[[NSString stringWithUTF8String:raw?:""]dataUsingEncoding:NSUTF8StringEncoding];
        NSDictionary *action=data?[NSJSONSerialization JSONObjectWithData:data options:0 error:nil]:nil;
        if(![action isKindOfClass:NSDictionary.class])return strdup("无效的应用窗口操作");
        NSDictionary *target=action[@"application"],*source=action[@"source"];if((id)source==NSNull.null)source=nil;
        NSString *kind=action[@"kind"],*bundle=target[@"bundle_id"],*path=target[@"app_path"];
        if(![path isKindOfClass:NSString.class]||![bundle isKindOfClass:NSString.class])return strdup("无法确认对应应用");
        NSURL *url=[NSURL fileURLWithPath:path];if(![[NSBundle bundleWithURL:url].bundleIdentifier isEqualToString:bundle])return strdup("对应应用已移动或身份已改变");
        if(source&&(!qy_same_process(source)||!qy_still_ancestor([source[@"pid"] intValue],[target[@"pid"] intValue])))return strdup("原会话进程已退出或所属窗口已改变");
        QYAgentOpenResult *result=[QYAgentOpenResult new];result.done=dispatch_semaphore_create(0);
        dispatch_async(dispatch_get_main_queue(),^{
            if(!qy_pending(result))return;
            if(source&&(!qy_same_process(source)||!qy_still_ancestor([source[@"pid"] intValue],[target[@"pid"] intValue]))){qy_finish(result,@"原会话进程已退出或编号已被复用");return;}
            if([kind isEqualToString:@"activate"]) {
                NSRunningApplication *app=qy_running(target);if(!app){qy_finish(result,@"对应的应用窗口已关闭");return;}
                BOOL success=[app activateWithOptions:NSApplicationActivateIgnoringOtherApps];
                qy_finish(result,success?nil:@"无法激活对应的应用窗口");
            } else if([kind isEqualToString:@"launch"]) {
                if(![bundle isEqualToString:@"com.openai.codex"]&&![bundle isEqualToString:@"com.anthropic.claudefordesktop"]){qy_finish(result,@"不支持启动此应用");return;}
                qy_open_application(url,result);
            } else if([kind isEqualToString:@"url"]&&[bundle isEqualToString:@"com.openai.codex"]) {
                NSString *link=action[@"url"];NSRegularExpression *valid=[NSRegularExpression regularExpressionWithPattern:@"^codex://threads/[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$" options:0 error:nil];
                if(![link isKindOfClass:NSString.class]||[valid numberOfMatchesInString:link options:0 range:NSMakeRange(0,link.length)]!=1){qy_finish(result,@"无效的 Codex 会话链接");return;}
                NSWorkspaceOpenConfiguration *configuration=NSWorkspaceOpenConfiguration.configuration;configuration.activates=YES;configuration.createsNewApplicationInstance=NO;
                [NSWorkspace.sharedWorkspace openURLs:@[[NSURL URLWithString:link]] withApplicationAtURL:url configuration:configuration completionHandler:^(NSRunningApplication *app,NSError *error){
                    if(!error){qy_finish(result,nil);return;}
                    dispatch_async(dispatch_get_main_queue(),^{if(!qy_pending(result))return;NSRunningApplication *fallback=qy_running(target);if(fallback&&[fallback activateWithOptions:NSApplicationActivateIgnoringOtherApps])qy_finish(result,nil);else qy_open_application(url,result);});
                }];
            } else qy_finish(result,@"不支持此应用窗口操作");
        });
        if(dispatch_semaphore_wait(result.done,dispatch_time(DISPATCH_TIME_NOW,4*NSEC_PER_SEC))!=0) {
            @synchronized(result){result.finished=YES;}return strdup("应用窗口定位超时，请重试");
        }
        if(result.error.length)return strdup(result.error.UTF8String);
        if([kind isEqualToString:@"activate"]&&source&&qy_same_process(source)&&qy_running(target)) {
            if(![bundle isEqualToString:@"com.apple.Terminal"]||!qy_terminal_tab(action[@"tty"]))qy_raise_existing_window([target[@"pid"] intValue],action[@"session_id"]);
        }
        return strdup("");
    }
}
