// Native macOS shell. AppKit entry points run on the event-loop thread; only
// qy_system_sample / qy_audio_command are called by the sampling worker.
#import <AppKit/AppKit.h>
#import <CoreAudio/CoreAudio.h>
#import <CoreGraphics/CoreGraphics.h>
#import <objc/runtime.h>
#import <QuartzCore/QuartzCore.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>
#include <stdatomic.h>
#include <math.h>

typedef struct { double x, y, w, h; } QYRect;
typedef struct {
    QYRect frame, work, camera;
    double scale, safe_top;
    uint32_t display_id, primary;
    char name[256], identifier[128];
} QYScreen;
typedef struct {
    int32_t battery, charging, volume, muted, network, locked;
    uint32_t audio_device;
    double idle;
    int32_t fullscreen;
    QYRect fullscreen_rect;
} QYSystem;
typedef void (*QYWake)(void *);
static _Atomic(bool) sessionLocked = false;
static double primary_top(void) {
    return NSMaxY([NSScreen screens].firstObject.frame);
}
static QYRect top_rect(NSRect r) {
    return (QYRect){r.origin.x, primary_top()-NSMaxY(r),r.size.width,r.size.height};
}
static NSRect cocoa_rect(QYRect r) {
    return NSMakeRect(r.x,primary_top()-r.y-r.h,r.w,r.h);
}
static BOOL in_rect(NSPoint p,QYRect r) {
    return r.w>0 && r.h>0 && p.x>=r.x && p.x<r.x+r.w && p.y>=r.y && p.y<r.y+r.h;
}
static BOOL qy_overlaps_menu_bar(QYRect surface,QYRect window,double scale,QYRect screen,double workTop) {
    if(surface.w<=0 || surface.h<=0 || scale<=0 || workTop<=screen.y) return NO;
    NSRect content=NSMakeRect(window.x+surface.x*scale,window.y+surface.y*scale,surface.w*scale,surface.h*scale);
    NSRect bar=NSMakeRect(screen.x,screen.y,screen.w,workTop-screen.y);
    return NSIntersectsRect(content,bar);
}

// Drag pasteboard contents outlive their dragging session. Only a new file
// payload written after this primary-button press may open the wider drop zone.
// The observation carries metadata only; no file paths or clipboard text are read.
typedef struct { NSInteger changeCount; BOOL containsFiles; } QYDragObservation;
typedef struct { BOOL primaryDown; NSInteger baseline; } QYFileDragState;
static BOOL qy_file_drag_step(QYFileDragState *state,BOOL down,QYDragObservation observation) {
    if(!down) {state->primaryDown=NO;return NO;}
    if(!state->primaryDown) {
        state->primaryDown=YES;state->baseline=observation.changeCount;return NO;
    }
    return observation.changeCount!=state->baseline && observation.containsFiles;
}
static BOOL qy_drag_types_contain_files(NSArray<NSPasteboardType> *types) {
    // Winit's destination accepts the legacy filenames representation, which
    // AppKit also supplies for modern file URLs. Plain URLs/text are not files.
    return [types containsObject:NSPasteboardTypeFileURL] || [types containsObject:@"NSFilenamesPboardType"];
}
typedef struct { BOOL inside,drop,captured,accept; } QYMouseDecision;
static QYMouseDecision qy_mouse_decision(NSPoint point,BOOL down,BOOL freshFiles,BOOL captured,BOOL hidden,QYRect hit,QYRect notice,QYRect dropZone) {
    QYMouseDecision result={0};
    result.captured=captured && down;
    result.inside=!hidden && (in_rect(point,hit)||in_rect(point,notice));
    result.drop=!hidden && down && freshFiles && in_rect(point,dropZone);
    result.accept=!hidden && (result.inside||result.drop||result.captured);
    return result;
}

@interface QYShell : NSObject <NSDraggingSource> {
    QYFileDragState _fileDrag;
}
@property(strong) NSWindow *window;
@property(strong) NSMenu *previousMainMenu;
@property(strong) NSStatusItem *status;
@property(strong) NSTimer *timer;
@property(strong) id localMonitor;
@property(strong) id globalMonitor;
@property(strong) id resignObserver;
@property(strong) id activeObserver;
@property(strong) id appObserver;
@property(strong) NSOpenPanel *panel;
@property(strong) NSWindow *blurWindow;
@property(strong) NSVisualEffectView *blurView;
@property(strong) CAShapeLayer *blurMask;
@property BOOL blurEnabled;
@property(strong) NSMutableArray<NSDictionary *> *pickerResults;
@property QYWake wake;
@property void *context;
@property uint32_t events;
@property BOOL editing;
@property BOOL hidden;
@property BOOL captured;
@property BOOL entered;
@property BOOL fileDropActive;
@property(strong) NSPasteboard *dragPasteboard;
@property NSInteger dragTypeCount;
@property BOOL dragHasFiles;
@property BOOL dragTypesKnown;
@property QYRect hit;
@property(strong) NSBezierPath *hitOutline;
@property QYRect notice;
@property(strong) NSBezierPath *noticeOutline;
@property QYRect dropZone;
@property double scale;
@property NSPoint lastMouse;
- (void)checkMouse;
- (void)mouseButtonEvent:(NSEvent *)event;
- (void)updateMouseAt:(NSPoint)point primaryDown:(BOOL)down drag:(QYDragObservation)observation;
- (void)event:(NSMenuItem *)sender;
- (void)edit:(NSMenuItem *)sender;
@end
static const void *QYShellKey = &QYShellKey;
static const void *QYUnconstrainedFrameKey = &QYUnconstrainedFrameKey;
static NSMutableDictionary<NSString *,NSValue *> *originalKeyMethods;
static NSMutableDictionary<NSString *,NSValue *> *originalMainMethods;
static NSMutableDictionary<NSString *,NSValue *> *originalConstrainMethods;
static IMP original_method(id instance,NSMutableDictionary<NSString *,NSValue *> *methods) {
    for(Class cls=object_getClass(instance);cls;cls=class_getSuperclass(cls)) {
        NSValue *method=methods[NSStringFromClass(cls)];if(method) return (IMP)method.pointerValue;
    }
    return NULL;
}
static BOOL canBecomeKey(id self, SEL cmd) {
    QYShell *shell = objc_getAssociatedObject(self,QYShellKey);
    if(shell) return shell.editing;
    IMP original=original_method(self,originalKeyMethods);
    return original?((BOOL(*)(id,SEL))original)(self,cmd):YES;
}
static BOOL canBecomeMain(id self, SEL cmd) {
    if(objc_getAssociatedObject(self,QYShellKey)) return NO;
    IMP original=original_method(self,originalMainMethods);
    return original?((BOOL(*)(id,SEL))original)(self,cmd):YES;
}
static NSRect constrainIslandFrame(id self,SEL cmd,NSRect frame,NSScreen *screen) {
    // The Rust geometry already bounds the visible resident/panel. AppKit's
    // ordinary window constraint instead bounds the transparent canvas and
    // silently moves a top-edge resident down to the menu bar.
    if(objc_getAssociatedObject(self,QYShellKey)||objc_getAssociatedObject(self,QYUnconstrainedFrameKey))return frame;
    IMP original=original_method(self,originalConstrainMethods);
    return original?((NSRect(*)(id,SEL,NSRect,NSScreen *))original)(self,cmd,frame,screen):frame;
}
static void allowIslandCanvasFrame(NSWindow *window) {
    if(!originalConstrainMethods)originalConstrainMethods=[NSMutableDictionary new];
    Class base=object_getClass(window);NSString *name=NSStringFromClass(base);
    Method method=class_getInstanceMethod(base,@selector(constrainFrameRect:toScreen:));IMP original=method_getImplementation(method);
    if(original!=(IMP)constrainIslandFrame&&!originalConstrainMethods[name]) {
        originalConstrainMethods[name]=[NSValue valueWithPointer:(void *)original];
        class_replaceMethod(base,@selector(constrainFrameRect:toScreen:),(IMP)constrainIslandFrame,method_getTypeEncoding(method));
    }
}
@implementation QYShell
- (void)event:(NSMenuItem *)sender {
    self.events |= (uint32_t)sender.tag;
    if (self.wake) self.wake(self.context);
}
- (void)edit:(NSMenuItem *)sender {
    NSWindow *window=NSApp.keyWindow;
    if(!window || !window.firstResponder) return;
    if([window.identifier hasPrefix:@"qingyu.qqmusic."]) {
        // WebKit uses native editing commands; egui windows retain their
        // keyboard-event path for text input and IME handling.
        SEL action=NULL;
        switch(sender.tag) {
            case 6: action=(sender.keyEquivalentModifierMask&NSEventModifierFlagShift)?@selector(redo:):@selector(undo:);break;
            case 7: action=@selector(cut:);break;
            case 8: action=@selector(copy:);break;
            case 9: action=@selector(paste:);break;
            case 0: action=@selector(selectAll:);break;
        }
        if(action) [NSApp sendAction:action to:nil from:sender];
        return;
    }
    NSString *character=sender.keyEquivalent;
    NSEvent *event=[NSEvent keyEventWithType:NSEventTypeKeyDown location:NSZeroPoint modifierFlags:sender.keyEquivalentModifierMask
        timestamp:NSProcessInfo.processInfo.systemUptime windowNumber:window.windowNumber context:nil characters:character charactersIgnoringModifiers:character isARepeat:NO keyCode:(unsigned short)sender.tag];
    [window.firstResponder keyDown:event];
}
- (void)mouseButtonEvent:(NSEvent *)event {
    if(event.type==NSEventTypeLeftMouseDown) {
        // Observe every new press, including an up/down pair between timer
        // ticks. A completed drag's unchanged payload is never a new session.
        _fileDrag=(QYFileDragState){YES,self.dragPasteboard.changeCount};
        self.dragTypesKnown=NO;
        self.captured=event.window==self.window;
    } else if(event.type==NSEventTypeLeftMouseUp) {
        _fileDrag.primaryDown=NO;self.captured=NO;
    }
}
- (void)checkMouse {
    if (!self.window) return;
    NSPoint point = [self.window.contentView convertPoint:[self.window convertPointFromScreen:NSEvent.mouseLocation] fromView:nil];
    if (!self.window.contentView.isFlipped) point.y = self.window.contentView.bounds.size.height-point.y;
    point.x /= self.scale; point.y /= self.scale;
    BOOL down = (NSEvent.pressedMouseButtons & 1) != 0;
    QYDragObservation observation={_fileDrag.baseline,NO};
    if(down) {
        observation.changeCount=self.dragPasteboard.changeCount;
        if(_fileDrag.primaryDown && observation.changeCount!=_fileDrag.baseline) {
            // Cache file types once known; a source can declare its types after
            // clearing the board, so retry an empty declaration while held.
            if(!self.dragTypesKnown || self.dragTypeCount!=observation.changeCount || !self.dragHasFiles) {
                self.dragTypeCount=observation.changeCount;
                self.dragHasFiles=qy_drag_types_contain_files(self.dragPasteboard.types);
                self.dragTypesKnown=YES;
            }
            observation.containsFiles=self.dragHasFiles;
        }
    }
    [self updateMouseAt:point primaryDown:down drag:observation];
}
- (void)updateMouseAt:(NSPoint)point primaryDown:(BOOL)down drag:(QYDragObservation)observation {
    BOOL freshFiles=qy_file_drag_step(&_fileDrag,down,observation);
    QYRect hit=self.hitOutline && ![self.hitOutline containsPoint:point]?(QYRect){0}:self.hit;
    QYRect notice=self.noticeOutline && ![self.noticeOutline containsPoint:point]?(QYRect){0}:self.notice;
    QYMouseDecision decision=qy_mouse_decision(point,down,freshFiles,self.captured,self.hidden,hit,notice,self.dropZone);
    self.captured=decision.captured;
    if(self.window.ignoresMouseEvents==decision.accept) self.window.ignoresMouseEvents=!decision.accept;
    // Wake for a file payload arriving under a stationary pointer as well as
    // ordinary hover movement. Other background drags never redraw the island.
    if(decision.drop!=self.fileDropActive || (!NSEqualPoints(point,self.lastMouse) && (decision.inside||self.entered||decision.captured||decision.drop))) {
        if(self.wake) self.wake(self.context);
    }
    self.entered=decision.inside;self.fileDropActive=decision.drop;self.lastMouse=point;
}
- (NSDragOperation)draggingSession:(NSDraggingSession *)session sourceOperationMaskForDraggingContext:(NSDraggingContext)context { return NSDragOperationCopy; }
- (BOOL)ignoreModifierKeysForDraggingSession:(NSDraggingSession *)session { return NO; }
@end

void *qy_shell_create(void *viewPtr,QYWake wake,void *context) {
    @autoreleasepool {
        NSView *view = (__bridge NSView *)viewPtr;
        if (!view.window) return NULL;
        QYShell *shell=[QYShell new];
        shell.window=view.window; shell.wake=wake; shell.context=context; shell.scale=1;
        shell.pickerResults=[NSMutableArray new];
        shell.dragPasteboard=[NSPasteboard pasteboardWithName:NSPasteboardNameDrag];
        NSWindow *window=shell.window;
        // Keep winit's class and AppKit's KVO isa intact. Replacing the isa
        // breaks AppKit observer cleanup on deallocation. Intercept only these
        // methods; every other window delegates to its original implementation.
        Class base=object_getClass(window);
        if(!originalKeyMethods) {originalKeyMethods=[NSMutableDictionary new];originalMainMethods=[NSMutableDictionary new];}
        NSString *className=NSStringFromClass(base);
        if(!originalKeyMethods[className]) {
            Method key=class_getInstanceMethod(base,@selector(canBecomeKeyWindow));
            Method main=class_getInstanceMethod(base,@selector(canBecomeMainWindow));
            IMP keyIMP=method_getImplementation(key),mainIMP=method_getImplementation(main);
            if(keyIMP!=(IMP)canBecomeKey) {
                originalKeyMethods[className]=[NSValue valueWithPointer:(void *)keyIMP];
                class_replaceMethod(base,@selector(canBecomeKeyWindow),(IMP)canBecomeKey,method_getTypeEncoding(key));
            }
            if(mainIMP!=(IMP)canBecomeMain) {
                originalMainMethods[className]=[NSValue valueWithPointer:(void *)mainIMP];
                class_replaceMethod(base,@selector(canBecomeMainWindow),(IMP)canBecomeMain,method_getTypeEncoding(main));
            }
        }
        objc_setAssociatedObject(window,QYShellKey,shell,OBJC_ASSOCIATION_ASSIGN);
        allowIslandCanvasFrame(window);
        window.opaque=NO; window.backgroundColor=NSColor.clearColor;
        window.hasShadow=NO; window.hidesOnDeactivate=NO;
        window.level=NSFloatingWindowLevel;
        window.collectionBehavior=NSWindowCollectionBehaviorCanJoinAllSpaces|NSWindowCollectionBehaviorFullScreenAuxiliary|NSWindowCollectionBehaviorStationary|NSWindowCollectionBehaviorIgnoresCycle;
        window.acceptsMouseMovedEvents=YES; window.ignoresMouseEvents=YES;
        [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];
        shell.status=[NSStatusBar.systemStatusBar statusItemWithLength:NSSquareStatusItemLength];
        NSImage *image=[[NSImage alloc] initWithContentsOfFile:[NSBundle.mainBundle pathForResource:@"qingyu-status" ofType:@"png"]];
        if(!image) image=[NSImage imageWithSystemSymbolName:@"square.stack.3d.up.fill" accessibilityDescription:@"轻屿"];
        image.size=NSMakeSize(18,18); image.template=YES; shell.status.button.image=image; shell.status.button.toolTip=@"轻屿 · 随手可用的桌面空间";
        NSMenu *menu=[[NSMenu alloc] initWithTitle:@"轻屿"];
        NSArray *labels=@[@"设置…",@"登录时启动",@"退出轻屿"];
        uint32_t tags[]={1,16,32};
        for(NSUInteger i=0;i<labels.count;i++) {
            if(i==2) [menu addItem:NSMenuItem.separatorItem];
            NSMenuItem *item=[[NSMenuItem alloc] initWithTitle:labels[i] action:@selector(event:) keyEquivalent:i==0?@",":i==2?@"q":@""];
            item.target=shell; item.tag=tags[i]; [menu addItem:item];
        }
        shell.status.menu=menu;
        shell.previousMainMenu=NSApp.mainMenu;
        NSMenu *menubar=[[NSMenu alloc] initWithTitle:@""];
        NSMenuItem *appHeading=[[NSMenuItem alloc] initWithTitle:@"轻屿" action:nil keyEquivalent:@""];
        NSMenu *appMenu=[[NSMenu alloc] initWithTitle:@"轻屿"];
        NSMenuItem *settings=[[NSMenuItem alloc] initWithTitle:@"设置…" action:@selector(event:) keyEquivalent:@","];
        settings.target=shell;settings.tag=1;[appMenu addItem:settings];
        [appMenu addItem:NSMenuItem.separatorItem];
        [appMenu addItemWithTitle:@"隐藏轻屿" action:@selector(hide:) keyEquivalent:@"h"];
        NSMenuItem *quit=[[NSMenuItem alloc] initWithTitle:@"退出轻屿" action:@selector(event:) keyEquivalent:@"q"];
        quit.target=shell;quit.tag=32;[appMenu addItem:quit];
        appHeading.submenu=appMenu;[menubar addItem:appHeading];
        NSMenuItem *fileHeading=[[NSMenuItem alloc] initWithTitle:@"文件" action:nil keyEquivalent:@""];
        NSMenu *fileMenu=[[NSMenu alloc] initWithTitle:@"文件"];
        [fileMenu addItemWithTitle:@"关闭窗口" action:@selector(performClose:) keyEquivalent:@"w"];
        fileHeading.submenu=fileMenu;[menubar addItem:fileHeading];
        NSMenuItem *musicHeading=[[NSMenuItem alloc] initWithTitle:@"QQ 音乐" action:nil keyEquivalent:@""];
        NSMenu *musicMenu=[[NSMenu alloc] initWithTitle:@"QQ 音乐"];
        NSArray *musicLabels=@[@"我的音乐…",@"播放器…"];uint32_t musicTags[]={128,256};
        for(NSUInteger i=0;i<musicLabels.count;i++) {
            NSMenuItem *item=[[NSMenuItem alloc] initWithTitle:musicLabels[i] action:@selector(event:) keyEquivalent:@""];
            item.target=shell;item.tag=musicTags[i];[musicMenu addItem:item];
        }
        musicHeading.submenu=musicMenu;[menubar addItem:musicHeading];
        NSMenuItem *editHeading=[[NSMenuItem alloc] initWithTitle:@"编辑" action:nil keyEquivalent:@""];
        NSMenu *editMenu=[[NSMenu alloc] initWithTitle:@"编辑"];
        NSArray *editLabels=@[@"撤销",@"重做",@"剪切",@"复制",@"粘贴",@"全选"];
        NSArray *editKeys=@[@"z",@"z",@"x",@"c",@"v",@"a"];NSInteger keyCodes[]={6,6,7,8,9,0};
        for(NSUInteger i=0;i<editLabels.count;i++) {
            if(i==2)[editMenu addItem:NSMenuItem.separatorItem];
            NSMenuItem *item=[[NSMenuItem alloc] initWithTitle:editLabels[i] action:@selector(edit:) keyEquivalent:editKeys[i]];
            item.target=shell;item.tag=keyCodes[i];
            if(i==1)item.keyEquivalentModifierMask=NSEventModifierFlagCommand|NSEventModifierFlagShift;
            [editMenu addItem:item];
        }
        editHeading.submenu=editMenu;[menubar addItem:editHeading];
        NSMenuItem *windowHeading=[[NSMenuItem alloc] initWithTitle:@"窗口" action:nil keyEquivalent:@""];
        NSMenu *windowMenu=[[NSMenu alloc] initWithTitle:@"窗口"];
        [windowMenu addItemWithTitle:@"最小化" action:@selector(performMiniaturize:) keyEquivalent:@"m"];
        [windowMenu addItemWithTitle:@"缩放" action:@selector(performZoom:) keyEquivalent:@""];
        windowHeading.submenu=windowMenu;[menubar addItem:windowHeading];
        NSApp.mainMenu=menubar;NSApp.windowsMenu=windowMenu;

        __weak QYShell *weak=shell;
        shell.timer=[NSTimer timerWithTimeInterval:1.0/60.0 repeats:YES block:^(NSTimer *timer){[weak checkMouse];}];
        [NSRunLoop.mainRunLoop addTimer:shell.timer forMode:NSRunLoopCommonModes];
        shell.localMonitor=[NSEvent addLocalMonitorForEventsMatchingMask:NSEventMaskLeftMouseDown|NSEventMaskLeftMouseUp|NSEventMaskMouseMoved|NSEventMaskLeftMouseDragged handler:^NSEvent *(NSEvent *event){
            QYShell *s=weak;
            [s mouseButtonEvent:event];
            [s checkMouse]; return event;
        }];
        shell.globalMonitor=[NSEvent addGlobalMonitorForEventsMatchingMask:NSEventMaskLeftMouseDown|NSEventMaskMouseMoved|NSEventMaskLeftMouseDragged|NSEventMaskLeftMouseUp handler:^(NSEvent *event){
            QYShell *s=weak;[s mouseButtonEvent:event];[s checkMouse];
        }];
        NSNotificationCenter *center=NSWorkspace.sharedWorkspace.notificationCenter;
        shell.resignObserver=[center addObserverForName:NSWorkspaceSessionDidResignActiveNotification object:nil queue:NSOperationQueue.mainQueue usingBlock:^(NSNotification *n){atomic_store(&sessionLocked,true);}];
        shell.activeObserver=[center addObserverForName:NSWorkspaceSessionDidBecomeActiveNotification object:nil queue:NSOperationQueue.mainQueue usingBlock:^(NSNotification *n){atomic_store(&sessionLocked,false); if(weak.wake) weak.wake(weak.context);}];
        shell.appObserver=[center addObserverForName:NSWorkspaceDidActivateApplicationNotification object:nil queue:NSOperationQueue.mainQueue usingBlock:^(NSNotification *n){
            NSRunningApplication *app=n.userInfo[NSWorkspaceApplicationKey];
            if(app.processIdentifier!=getpid()) {weak.events|=64; if(weak.wake) weak.wake(weak.context);}
        }];
        return (__bridge_retained void *)shell;
    }
}
void qy_shell_destroy(void *ptr) {
    // The window association is also an owner. Keep the bridge's reference
    // alive until every observer, menu and child window is disconnected.
    __attribute__((objc_precise_lifetime)) QYShell *shell=(__bridge_transfer QYShell *)ptr;
    shell.wake=NULL;
    NSApp.mainMenu=shell.previousMainMenu;
    [shell.timer invalidate];
    if(shell.localMonitor) [NSEvent removeMonitor:shell.localMonitor];
    if(shell.globalMonitor) [NSEvent removeMonitor:shell.globalMonitor];
    NSNotificationCenter *center=NSWorkspace.sharedWorkspace.notificationCenter;
    for(id observer in @[shell.resignObserver,shell.activeObserver,shell.appObserver]) [center removeObserver:observer];
    [shell.panel cancel:nil];
    if(shell.blurWindow) {[shell.window removeChildWindow:shell.blurWindow];[shell.blurWindow orderOut:nil];objc_setAssociatedObject(shell.blurWindow,QYUnconstrainedFrameKey,nil,OBJC_ASSOCIATION_ASSIGN);}
    if(shell.status) [NSStatusBar.systemStatusBar removeStatusItem:shell.status];
    objc_setAssociatedObject(shell.window,QYShellKey,nil,OBJC_ASSOCIATION_ASSIGN);
}
uint32_t qy_shell_events(void *ptr) {
    QYShell *shell=(__bridge QYShell *)ptr; uint32_t value=shell.events; shell.events=0; return value;
}
void qy_shell_menu(void *ptr,int pet,int minimal,int autostart) {
    (void)pet; (void)minimal; // Kept in the ABI for existing host callers.
    QYShell *shell=(__bridge QYShell *)ptr;
    for(NSMenuItem *item in shell.status.menu.itemArray) {
        if(item.tag==16) item.state=autostart?NSControlStateValueOn:NSControlStateValueOff;
    }
}
void qy_shell_hit_outline(void *ptr,const double *xy,size_t count) {
    QYShell *shell=(__bridge QYShell *)ptr;
    if(count<3) {shell.hitOutline=nil;return;}
    NSBezierPath *path=[NSBezierPath bezierPath];
    [path moveToPoint:NSMakePoint(xy[0],xy[1])];
    for(size_t i=1;i<count;i++) [path lineToPoint:NSMakePoint(xy[i*2],xy[i*2+1])];
    [path closePath];shell.hitOutline=path;
}
void qy_shell_hit_outlines(void *ptr,const double *xy,const uint32_t *counts,size_t count) {
    QYShell *shell=(__bridge QYShell *)ptr;
    if(count==0) {shell.hitOutline=nil;return;}
    NSBezierPath *path=[NSBezierPath bezierPath];path.windingRule=NSWindingRuleNonZero;
    size_t offset=0;
    for(size_t shape=0;shape<count;shape++) {
        size_t points=counts[shape];
        if(points>=3) {
            [path moveToPoint:NSMakePoint(xy[offset*2],xy[offset*2+1])];
            for(size_t i=1;i<points;i++) [path lineToPoint:NSMakePoint(xy[(offset+i)*2],xy[(offset+i)*2+1])];
            [path closePath];
        }
        offset+=points;
    }
    shell.hitOutline=path;
}
void qy_shell_notice_outlines(void *ptr,const double *xy,const uint32_t *counts,size_t count) {
    QYShell *shell=(__bridge QYShell *)ptr;
    if(count==0) {shell.noticeOutline=nil;return;}
    NSBezierPath *path=[NSBezierPath bezierPath];
    path.windingRule=NSWindingRuleNonZero;
    size_t offset=0;
    for(size_t shape=0;shape<count;shape++) {
        size_t points=counts[shape];
        if(points>=3) {
            [path moveToPoint:NSMakePoint(xy[offset*2],xy[offset*2+1])];
            for(size_t i=1;i<points;i++) [path lineToPoint:NSMakePoint(xy[(offset+i)*2],xy[(offset+i)*2+1])];
            [path closePath];
        }
        offset+=points;
    }
    shell.noticeOutline=path;
}
void qy_shell_update(void *ptr,QYRect hit,QYRect notice,QYRect drop,double scale,int editing,int hidden) {
    QYShell *shell=(__bridge QYShell *)ptr;
    shell.hit=hit; shell.notice=notice; shell.dropZone=drop; shell.scale=scale;
    // Free placement can occupy the menu-bar strip. Raise only the visible
    // island/notice there; transparent pixels still pass every click through.
    BOOL overMenu=NO;
    if(!hidden) {
        QYRect window=top_rect(shell.window.frame);
        for(NSScreen *screen in NSScreen.screens) {
            QYRect frame=top_rect(screen.frame),work=top_rect(screen.visibleFrame);
            double top=fmax(work.y,frame.y+NSStatusBar.systemStatusBar.thickness);
            if(qy_overlaps_menu_bar(hit,window,scale,frame,top) || qy_overlaps_menu_bar(notice,window,scale,frame,top)) {overMenu=YES;break;}
        }
    }
    NSInteger level=overMenu?NSStatusWindowLevel+1:NSFloatingWindowLevel;
    if(shell.window.level!=level) {shell.window.level=level;shell.blurWindow.level=level;}
    if(editing && !shell.editing) {shell.editing=YES; [NSApp activateIgnoringOtherApps:YES]; [shell.window makeKeyWindow];}
    // Position drags temporarily opt into keyboard delivery for Escape. Once
    // that request ends, the transparent island must stop owning key events.
    if(!editing && shell.editing && shell.window.isKeyWindow) [shell.window resignKeyWindow];
    shell.editing=editing; shell.hidden=hidden;
    shell.window.alphaValue=hidden?0:1;
    shell.blurWindow.alphaValue=hidden||!shell.blurEnabled?0:1;
    [shell checkMouse];
}
void qy_shell_focus(void *ptr) {
    QYShell *shell=(__bridge QYShell *)ptr; shell.editing=YES;
    [NSApp activateIgnoringOtherApps:YES]; [shell.window makeKeyAndOrderFront:nil];
}
void qy_activate(void) { [NSApp activateIgnoringOtherApps:YES]; }
int qy_shell_geometry(void *ptr,QYRect *frame,double *scale) {
    QYShell *shell=(__bridge QYShell *)ptr; if(!shell.window) return 0;
    *frame=top_rect(shell.window.frame); *scale=shell.window.backingScaleFactor; return 1;
}
int qy_shell_place(void *ptr,QYRect frame) {
    QYShell *shell=(__bridge QYShell *)ptr; if(!shell.window || frame.w<=0 || frame.h<=0) return 0;
    NSRect native=cocoa_rect(frame);
    if(!NSEqualRects(native,shell.window.frame)) [shell.window setFrame:native display:NO];
    if(shell.blurWindow && !NSEqualRects(native,shell.blurWindow.frame)) [shell.blurWindow setFrame:native display:NO];
    [shell.window orderFrontRegardless]; return 1;
}
// A separate child window behind Metal avoids covering the rendered content
// with a visual-effect subview. Its mask contains only the visible island paths.
int qy_shell_blur(void *ptr,const double *xy,const uint32_t *counts,size_t paths) {
    QYShell *shell=(__bridge QYShell *)ptr;
    BOOL enabled=paths>0 && !NSWorkspace.sharedWorkspace.accessibilityDisplayShouldReduceTransparency;
    shell.blurEnabled=enabled;
    if(!enabled) {shell.blurWindow.alphaValue=0;return 0;}
    if(!shell.blurWindow) {
        NSWindow *window=[[NSWindow alloc] initWithContentRect:shell.window.frame styleMask:NSWindowStyleMaskBorderless backing:NSBackingStoreBuffered defer:NO];
        objc_setAssociatedObject(window,QYUnconstrainedFrameKey,@YES,OBJC_ASSOCIATION_RETAIN_NONATOMIC);allowIslandCanvasFrame(window);
        // AppKit closes child windows when the winit parent closes. Disable
        // NSWindow's extra close-time release because ARC owns this window;
        // otherwise blurWindow becomes a dangling strong property at teardown.
        window.releasedWhenClosed=NO;
        window.opaque=NO;window.backgroundColor=NSColor.clearColor;window.hasShadow=NO;
        window.ignoresMouseEvents=YES;window.hidesOnDeactivate=NO;window.level=shell.window.level;
        window.collectionBehavior=shell.window.collectionBehavior;
        NSVisualEffectView *view=[[NSVisualEffectView alloc] initWithFrame:NSMakeRect(0,0,shell.window.frame.size.width,shell.window.frame.size.height)];
        view.blendingMode=NSVisualEffectBlendingModeBehindWindow;view.material=NSVisualEffectMaterialUnderWindowBackground;view.state=NSVisualEffectStateActive;
        view.wantsLayer=YES;view.autoresizingMask=NSViewWidthSizable|NSViewHeightSizable;
        CAShapeLayer *mask=[CAShapeLayer layer];view.layer.mask=mask;
        window.contentView=view;shell.blurWindow=window;shell.blurView=view;shell.blurMask=mask;
        [shell.window addChildWindow:window ordered:NSWindowBelow];
    }
    CGMutablePathRef path=CGPathCreateMutable();size_t at=0;double height=shell.window.frame.size.height;
    for(size_t i=0;i<paths;i++) {
        for(uint32_t j=0;j<counts[i];j++,at++) {
            CGFloat x=xy[at*2]*shell.scale,y=height-xy[at*2+1]*shell.scale;
            if(j==0) CGPathMoveToPoint(path,NULL,x,y);else CGPathAddLineToPoint(path,NULL,x,y);
        }
        CGPathCloseSubpath(path);
    }
    [CATransaction begin];[CATransaction setDisableActions:YES];shell.blurMask.path=path;[CATransaction commit];CGPathRelease(path);
    shell.blurWindow.alphaValue=shell.hidden?0:1;return 1;
}
int qy_shell_cursor(void *ptr,double *x,double *y) {
    QYShell *shell=(__bridge QYShell *)ptr; if(!shell.window) return 0;
    NSView *view=shell.window.contentView;
    NSPoint point=[view convertPoint:[shell.window convertPointFromScreen:NSEvent.mouseLocation] fromView:nil];
    *x=point.x; *y=view.isFlipped?point.y:view.bounds.size.height-point.y; return 1;
}
int qy_shell_global_cursor(void *ptr,double *x,double *y) {
    QYShell *shell=(__bridge QYShell *)ptr; if(!shell.window) return 0;
    NSPoint point=NSEvent.mouseLocation;
    *x=point.x; *y=primary_top()-point.y; return 1;
}
size_t qy_screens(QYScreen *out,size_t capacity) {
    @autoreleasepool {
        NSArray<NSScreen *> *screens=NSScreen.screens; size_t count=MIN(capacity,screens.count);
        for(size_t i=0;i<count;i++) {
            NSScreen *screen=screens[i]; memset(&out[i],0,sizeof(QYScreen));
            out[i].frame=top_rect(screen.frame); out[i].work=top_rect(screen.visibleFrame);
            out[i].scale=screen.backingScaleFactor; out[i].primary=i==0;
            if(@available(macOS 12.0,*)) {
                out[i].safe_top=screen.safeAreaInsets.top;
                NSRect left=screen.auxiliaryTopLeftArea,right=screen.auxiliaryTopRightArea;
                CGFloat cameraLeft=NSMaxX(left),cameraRight=NSMinX(right);
                // Both auxiliary areas must be real. A reserved menu bar by
                // itself is not evidence of a physical camera housing.
                if(out[i].safe_top>0 && !NSIsEmptyRect(left) && !NSIsEmptyRect(right) && cameraRight>cameraLeft && cameraLeft>=NSMinX(screen.frame) && cameraRight<=NSMaxX(screen.frame)) {
                    out[i].camera=top_rect(NSMakeRect(cameraLeft,NSMaxY(screen.frame)-out[i].safe_top,cameraRight-cameraLeft,out[i].safe_top));
                }
            }
            out[i].display_id=[screen.deviceDescription[@"NSScreenNumber"] unsignedIntValue];
            snprintf(out[i].name,sizeof(out[i].name),"%s",screen.localizedName.UTF8String);
            CFUUIDRef uuid=CGDisplayCreateUUIDFromDisplayID(out[i].display_id);
            NSString *identifier=uuid?CFBridgingRelease(CFUUIDCreateString(NULL,uuid)):@"";
            if(uuid) CFRelease(uuid);
            snprintf(out[i].identifier,sizeof(out[i].identifier),"%s",identifier.UTF8String);
        }
        return count;
    }
}
int qy_open(const char *path,int reveal) {
    @autoreleasepool {
        NSString *string=[NSString stringWithUTF8String:path]; if(!string) return 0;
        if(reveal) { [NSWorkspace.sharedWorkspace activateFileViewerSelectingURLs:@[[NSURL fileURLWithPath:string]]]; return 1; }
        NSURL *url=([string hasPrefix:@"https://"]||[string hasPrefix:@"http://"])?[NSURL URLWithString:string]:[NSURL fileURLWithPath:string];
        return url && [NSWorkspace.sharedWorkspace openURL:url];
    }
}
int qy_pick_files(void *ptr,uint32_t kind,const char *title,int multiple) {
    QYShell *shell=(__bridge QYShell *)ptr; if(shell.panel) return 0;
    NSOpenPanel *panel=NSOpenPanel.openPanel; shell.panel=panel;
    panel.title=[NSString stringWithUTF8String:title]; panel.canChooseFiles=YES;
    panel.canChooseDirectories=kind==1; panel.allowsMultipleSelection=multiple;
    [NSApp activateIgnoringOtherApps:YES];
    __weak QYShell *weak=shell;
    [panel beginWithCompletionHandler:^(NSModalResponse response){
        QYShell *s=weak; if(!s) return;
        NSMutableArray *paths=[NSMutableArray new];
        if(response==NSModalResponseOK) for(NSURL *url in panel.URLs) [paths addObject:url.path];
        [s.pickerResults addObject:@{@"kind":@(kind),@"paths":paths}];
        s.panel=nil; if(s.wake) s.wake(s.context);
    }]; return 1;
}
char *qy_take_picker(void *ptr) {
    QYShell *shell=(__bridge QYShell *)ptr;
    NSDictionary *result=shell.pickerResults.firstObject; if(!result) return NULL;
    [shell.pickerResults removeObjectAtIndex:0];
    NSData *json=[NSJSONSerialization dataWithJSONObject:result options:0 error:nil];
    return json?strndup(json.bytes,json.length):NULL;
}
void qy_string_free(char *str) { free(str); }
int qy_drag_file(void *ptr,const char *path) {
    QYShell *shell=(__bridge QYShell *)ptr; NSEvent *event=NSApp.currentEvent;
    if(!event || event.window!=shell.window || !(NSEvent.pressedMouseButtons&1)) return 0;
    NSURL *url=[NSURL fileURLWithPath:[NSString stringWithUTF8String:path]];
    NSDraggingItem *item=[[NSDraggingItem alloc] initWithPasteboardWriter:url];
    NSPoint point=[shell.window.contentView convertPoint:event.locationInWindow fromView:nil];
    NSImage *image=[NSWorkspace.sharedWorkspace iconForFile:url.path];
    [item setDraggingFrame:NSMakeRect(point.x,point.y,48,48) contents:image];
    [shell.window.contentView beginDraggingSessionWithItems:@[item] event:event source:shell]; return 1;
}

static AudioDeviceID output_device(void) {
    AudioDeviceID device=kAudioObjectUnknown; UInt32 size=sizeof(device);
    AudioObjectPropertyAddress property={kAudioHardwarePropertyDefaultOutputDevice,kAudioObjectPropertyScopeGlobal,kAudioObjectPropertyElementMain};
    if(AudioObjectGetPropertyData(kAudioObjectSystemObject,&property,0,NULL,&size,&device)!=noErr) return kAudioObjectUnknown;
    return device;
}
static BOOL audio_float(AudioDeviceID device,AudioObjectPropertySelector selector,AudioObjectPropertyElement element,Float32 *value) {
    UInt32 size=sizeof(*value); AudioObjectPropertyAddress p={selector,kAudioDevicePropertyScopeOutput,element};
    return AudioObjectGetPropertyData(device,&p,0,NULL,&size,value)==noErr;
}
static BOOL audio_muted(AudioDeviceID device,UInt32 *value) {
    UInt32 size=sizeof(*value); AudioObjectPropertyAddress p={kAudioDevicePropertyMute,kAudioDevicePropertyScopeOutput,kAudioObjectPropertyElementMain};
    return AudioObjectGetPropertyData(device,&p,0,NULL,&size,value)==noErr;
}
static BOOL set_audio_float(AudioDeviceID device,AudioObjectPropertyElement element,Float32 value) {
    AudioObjectPropertyAddress p={kAudioDevicePropertyVolumeScalar,kAudioDevicePropertyScopeOutput,element}; Boolean settable=false;
    return AudioObjectIsPropertySettable(device,&p,&settable)==noErr && settable && AudioObjectSetPropertyData(device,&p,0,NULL,sizeof(value),&value)==noErr;
}
int qy_audio_command(int volume,int toggle) {
    AudioDeviceID device=output_device(); if(device==kAudioObjectUnknown) return 0;
    if(toggle) {
        UInt32 muted=0; if(!audio_muted(device,&muted)) return 0; muted=!muted;
        AudioObjectPropertyAddress p={kAudioDevicePropertyMute,kAudioDevicePropertyScopeOutput,kAudioObjectPropertyElementMain};
        return AudioObjectSetPropertyData(device,&p,0,NULL,sizeof(muted),&muted)==noErr;
    }
    Float32 value=MAX(0,MIN(100,volume))/100.0f;
    if(set_audio_float(device,kAudioObjectPropertyElementMain,value)) return 1;
    BOOL left=set_audio_float(device,1,value),right=set_audio_float(device,2,value); return left||right;
}
extern bool qy_notifications_session_locked(void);
void qy_system_sample(QYSystem *out) {
    @autoreleasepool {
        memset(out,0,sizeof(*out)); out->battery=-1; out->volume=-1; out->network=-1;
        out->locked=atomic_load(&sessionLocked)||qy_notifications_session_locked();
        AudioDeviceID device=output_device(); out->audio_device=device;
        Float32 volume=0,left=0,right=0; UInt32 mute=0;
        if(audio_float(device,kAudioDevicePropertyVolumeScalar,kAudioObjectPropertyElementMain,&volume)) out->volume=(int32_t)roundf(volume*100);
        else if(audio_float(device,kAudioDevicePropertyVolumeScalar,1,&left) && audio_float(device,kAudioDevicePropertyVolumeScalar,2,&right)) out->volume=(int32_t)roundf((left+right)*50);
        if(audio_muted(device,&mute)) out->muted=mute!=0;
        pid_t foreground=NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier;
        if(foreground>0 && foreground!=getpid()) {
            NSArray *windows=CFBridgingRelease(CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly|kCGWindowListExcludeDesktopElements,kCGNullWindowID));
            CGDirectDisplayID displays[32]; uint32_t count=0; CGGetActiveDisplayList(32,displays,&count);
            for(NSDictionary *window in windows) {
                if([window[(__bridge NSString *)kCGWindowOwnerPID] intValue]!=foreground || [window[(__bridge NSString *)kCGWindowLayer] intValue]!=0) continue;
                CGRect frame; if(!CGRectMakeWithDictionaryRepresentation((__bridge CFDictionaryRef)window[(__bridge NSString *)kCGWindowBounds],&frame)) continue;
                for(uint32_t i=0;i<count;i++) {
                    CGRect screen=CGDisplayBounds(displays[i]);
                    if(fabs(frame.origin.x-screen.origin.x)<=2 && fabs(frame.origin.y-screen.origin.y)<=2 && fabs(frame.size.width-screen.size.width)<=2 && fabs(frame.size.height-screen.size.height)<=2) {
                        out->fullscreen=1;out->fullscreen_rect=(QYRect){screen.origin.x,screen.origin.y,screen.size.width,screen.size.height};break;
                    }
                }
                // The first layer-zero window of the foreground app is its
                // front window. A full-screen window behind a dialog is not active.
                break;
            }
        }
    }
}
