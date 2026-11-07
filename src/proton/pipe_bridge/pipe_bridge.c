/*
 * AeroDeck Wine/Proton Named-Pipe-to-TCP Bridge (aerodeck-pipe-bridge.exe)
 *
 * Freestanding Win64 executable (no CRT dependency; links only kernel32.dll and ws2_32.dll).
 * Runs inside the active MSFS Steam Proton / Wine prefix to bridge Windows Named Pipes:
 *   - \\.\pipe\Custom\SimConnect
 *   - \\.\pipe\Microsoft Flight Simulator\SimConnect:0
 *   - \\.\pipe\Microsoft Flight Simulator\SimConnect
 * onto a standard loopback TCP socket (default 127.0.0.1:45454) for native Linux clients.
 */

typedef unsigned char      uint8_t;
typedef unsigned short     uint16_t;
typedef unsigned int       uint32_t;
typedef unsigned long long uint64_t;
typedef void*              HANDLE;
typedef uint64_t           SOCKET;

#define NULL ((void*)0)
#define INVALID_HANDLE_VALUE ((HANDLE)(long long)-1)
#define INVALID_SOCKET       ((SOCKET)(~0ULL))
#define SOCKET_ERROR         (-1)

#define GENERIC_READ         0x80000000U
#define GENERIC_WRITE        0x40000000U
#define OPEN_EXISTING        3U
#define FILE_FLAG_OVERLAPPED 0x40000000U
#define PIPE_ACCESS_DUPLEX   0x00000003U
#define PIPE_TYPE_BYTE       0x00000000U
#define PIPE_WAIT            0x00000000U

#define ERROR_IO_PENDING     997U
#define ERROR_PIPE_Connected 535U
#define WAIT_OBJECT_0        0x00000000U
#define INFINITE             0xFFFFFFFFU
#define STD_OUTPUT_HANDLE    ((uint32_t)-11)

#define AF_INET              2
#define SOCK_STREAM          1
#define IPPROTO_TCP          6
#define SOL_SOCKET           0xFFFF
#define SO_REUSEADDR         0x0004

typedef struct _OVERLAPPED {
    uint64_t Internal;
    uint64_t InternalHigh;
    union {
        struct {
            uint32_t Offset;
            uint32_t OffsetHigh;
        };
        void* Pointer;
    };
    HANDLE hEvent;
} OVERLAPPED;

typedef struct _WSADATA {
    uint16_t wVersion;
    uint16_t wHighVersion;
    uint16_t iMaxSockets;
    uint16_t iMaxUdpDg;
    char*    lpVendorInfo;
    char     szDescription[257];
    char     szSystemStatus[129];
} WSADATA;

typedef struct _sockaddr_in {
    short          sin_family;
    uint16_t       sin_port;
    uint32_t       sin_addr;
    unsigned char  sin_zero[8];
} sockaddr_in;

/* kernel32.dll imports */
__declspec(dllimport) HANDLE   __stdcall CreateEventA(void*, int, int, const char*);
__declspec(dllimport) HANDLE   __stdcall CreateFileA(const char*, uint32_t, uint32_t, void*, uint32_t, uint32_t, HANDLE);
__declspec(dllimport) HANDLE   __stdcall CreateNamedPipeA(const char*, uint32_t, uint32_t, uint32_t, uint32_t, uint32_t, uint32_t, void*);
__declspec(dllimport) int      __stdcall ConnectNamedPipe(HANDLE, OVERLAPPED*);
__declspec(dllimport) HANDLE   __stdcall CreateThread(void*, uint64_t, uint32_t (__stdcall *)(void*), void*, uint32_t, uint32_t*);
__declspec(dllimport) int      __stdcall CloseHandle(HANDLE);
__declspec(dllimport) void     __stdcall ExitProcess(uint32_t);
__declspec(dllimport) char*    __stdcall GetCommandLineA(void);
__declspec(dllimport) uint32_t __stdcall GetLastError(void);
__declspec(dllimport) int      __stdcall GetOverlappedResult(HANDLE, OVERLAPPED*, uint32_t*, int);
__declspec(dllimport) int      __stdcall ReadFile(HANDLE, void*, uint32_t, uint32_t*, OVERLAPPED*);
__declspec(dllimport) int      __stdcall ResetEvent(HANDLE);
__declspec(dllimport) void     __stdcall Sleep(uint32_t);
__declspec(dllimport) uint32_t __stdcall WaitForSingleObject(HANDLE, uint32_t);
__declspec(dllimport) int      __stdcall WriteFile(HANDLE, const void*, uint32_t, uint32_t*, OVERLAPPED*);
__declspec(dllimport) HANDLE   __stdcall GetStdHandle(uint32_t);

/* ws2_32.dll imports */
__declspec(dllimport) int      __stdcall WSAStartup(uint16_t, WSADATA*);
__declspec(dllimport) int      __stdcall WSACleanup(void);
__declspec(dllimport) SOCKET   __stdcall socket(int, int, int);
__declspec(dllimport) int      __stdcall bind(SOCKET, const sockaddr_in*, int);
__declspec(dllimport) int      __stdcall listen(SOCKET, int);
__declspec(dllimport) SOCKET   __stdcall accept(SOCKET, void*, int*);
__declspec(dllimport) int      __stdcall connect(SOCKET, const sockaddr_in*, int);
__declspec(dllimport) int      __stdcall recv(SOCKET, char*, int, int);
__declspec(dllimport) int      __stdcall send(SOCKET, const char*, int, int);
__declspec(dllimport) int      __stdcall closesocket(SOCKET);
__declspec(dllimport) int      __stdcall setsockopt(SOCKET, int, int, const char*, int);
__declspec(dllimport) uint16_t __stdcall htons(uint16_t);
__declspec(dllimport) uint32_t __stdcall inet_addr(const char*);

static const char* DEFAULT_PIPES[] = {
    "\\\\.\\pipe\\Custom\\SimConnect",
    "\\\\.\\pipe\\Microsoft Flight Simulator\\SimConnect:0",
    "\\\\.\\pipe\\Microsoft Flight Simulator\\SimConnect",
    NULL
};

static const char* g_custom_pipe = NULL;

static uint32_t str_len(const char* s) {
    uint32_t n = 0;
    while (s && s[n]) n++;
    return n;
}

static int str_contains(const char* haystack, const char* needle) {
    uint32_t hlen = str_len(haystack);
    uint32_t nlen = str_len(needle);
    if (nlen == 0 || nlen > hlen) return 0;
    for (uint32_t i = 0; i <= hlen - nlen; i++) {
        uint32_t j = 0;
        while (j < nlen && haystack[i + j] == needle[j]) j++;
        if (j == nlen) return 1;
    }
    return 0;
}

static const char* str_find_after(const char* haystack, const char* flag) {
    uint32_t hlen = str_len(haystack);
    uint32_t flen = str_len(flag);
    if (flen == 0 || flen >= hlen) return NULL;
    for (uint32_t i = 0; i <= hlen - flen; i++) {
        uint32_t j = 0;
        while (j < flen && haystack[i + j] == flag[j]) j++;
        if (j == flen) {
            const char* p = haystack + i + flen;
            while (*p == ' ' || *p == '=') p++;
            return p;
        }
    }
    return NULL;
}

static uint16_t parse_u16(const char* s, uint16_t fallback) {
    if (!s || *s < '0' || *s > '9') return fallback;
    uint32_t val = 0;
    while (*s >= '0' && *s <= '9') {
        val = val * 10 + (uint32_t)(*s - '0');
        s++;
    }
    return (val > 0 && val <= 65535) ? (uint16_t)val : fallback;
}

static void print_str(const char* s) {
    HANDLE out = GetStdHandle(STD_OUTPUT_HANDLE);
    if (out != INVALID_HANDLE_VALUE && out != NULL) {
        uint32_t written = 0;
        WriteFile(out, s, str_len(s), &written, NULL);
    }
}

static void zero_mem(void* ptr, uint32_t len) {
    unsigned char* p = (unsigned char*)ptr;
    for (uint32_t i = 0; i < len; i++) p[i] = 0;
}

static HANDLE open_simconnect_pipe(void) {
    if (g_custom_pipe) {
        return CreateFileA(
            g_custom_pipe,
            GENERIC_READ | GENERIC_WRITE,
            0,
            NULL,
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED,
            NULL
        );
    }
    for (int i = 0; DEFAULT_PIPES[i] != NULL; i++) {
        HANDLE h = CreateFileA(
            DEFAULT_PIPES[i],
            GENERIC_READ | GENERIC_WRITE,
            0,
            NULL,
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED,
            NULL
        );
        if (h != INVALID_HANDLE_VALUE) {
            return h;
        }
    }
    return INVALID_HANDLE_VALUE;
}

typedef struct _BridgeSession {
    SOCKET sock;
    HANDLE pipe;
    volatile int active;
} BridgeSession;

static BridgeSession g_sessions[16];
static int g_session_idx = 0;

/* Thread 1: Read from Named Pipe (Overlapped) -> Send to TCP Socket */
static uint32_t __stdcall pipe_to_tcp_thread(void* arg) {
    BridgeSession* s = (BridgeSession*)arg;
    char buf[16384];
    OVERLAPPED ov;
    zero_mem(&ov, sizeof(ov));
    ov.hEvent = CreateEventA(NULL, 1, 0, NULL);

    while (s->active) {
        ResetEvent(ov.hEvent);
        uint32_t bytes_read = 0;
        int ok = ReadFile(s->pipe, buf, sizeof(buf), &bytes_read, &ov);
        if (!ok) {
            uint32_t err = GetLastError();
            if (err == ERROR_IO_PENDING) {
                WaitForSingleObject(ov.hEvent, INFINITE);
                if (!GetOverlappedResult(s->pipe, &ov, &bytes_read, 0)) {
                    break;
                }
            } else {
                break;
            }
        }
        if (bytes_read == 0) {
            break;
        }
        uint32_t total_sent = 0;
        while (total_sent < bytes_read) {
            int n = send(s->sock, buf + total_sent, (int)(bytes_read - total_sent), 0);
            if (n <= 0) {
                s->active = 0;
                break;
            }
            total_sent += (uint32_t)n;
        }
    }

    s->active = 0;
    CloseHandle(ov.hEvent);
    closesocket(s->sock);
    return 0;
}

/* Thread 2: Read from TCP Socket -> Write to Named Pipe (Overlapped) */
static uint32_t __stdcall tcp_to_pipe_thread(void* arg) {
    BridgeSession* s = (BridgeSession*)arg;
    char buf[16384];
    OVERLAPPED ov;
    zero_mem(&ov, sizeof(ov));
    ov.hEvent = CreateEventA(NULL, 1, 0, NULL);

    while (s->active) {
        int n = recv(s->sock, buf, sizeof(buf), 0);
        if (n <= 0) {
            break;
        }
        uint32_t total_written = 0;
        while (total_written < (uint32_t)n) {
            ResetEvent(ov.hEvent);
            uint32_t chunk_written = 0;
            int ok = WriteFile(
                s->pipe,
                buf + total_written,
                (uint32_t)n - total_written,
                &chunk_written,
                &ov
            );
            if (!ok) {
                uint32_t err = GetLastError();
                if (err == ERROR_IO_PENDING) {
                    WaitForSingleObject(ov.hEvent, INFINITE);
                    if (!GetOverlappedResult(s->pipe, &ov, &chunk_written, 0)) {
                        s->active = 0;
                        break;
                    }
                } else {
                    s->active = 0;
                    break;
                }
            }
            if (chunk_written == 0) {
                s->active = 0;
                break;
            }
            total_written += chunk_written;
        }
    }

    s->active = 0;
    CloseHandle(ov.hEvent);
    CloseHandle(s->pipe);
    closesocket(s->sock);
    return 0;
}

/* Self-test mock pipe server thread */
static uint32_t __stdcall mock_pipe_server_thread(void* arg) {
    HANDLE hPipe = (HANDLE)arg;
    OVERLAPPED ov;
    zero_mem(&ov, sizeof(ov));
    ov.hEvent = CreateEventA(NULL, 1, 0, NULL);

    int connected = ConnectNamedPipe(hPipe, &ov);
    if (!connected) {
        uint32_t err = GetLastError();
        if (err == ERROR_IO_PENDING) {
            WaitForSingleObject(ov.hEvent, 5000);
        } else if (err != ERROR_PIPE_Connected) {
            CloseHandle(ov.hEvent);
            CloseHandle(hPipe);
            return 1;
        }
    }

    char buf[64];
    uint32_t read_bytes = 0;
    ResetEvent(ov.hEvent);
    if (!ReadFile(hPipe, buf, sizeof(buf), &read_bytes, &ov)) {
        if (GetLastError() == ERROR_IO_PENDING) {
            WaitForSingleObject(ov.hEvent, 5000);
            GetOverlappedResult(hPipe, &ov, &read_bytes, 0);
        }
    }

    /* Echo back a 16-byte response header */
    const char reply[16] = "SIMCONNECT_OK!!\n";
    uint32_t written = 0;
    ResetEvent(ov.hEvent);
    if (!WriteFile(hPipe, reply, 16, &written, &ov)) {
        if (GetLastError() == ERROR_IO_PENDING) {
            WaitForSingleObject(ov.hEvent, 5000);
            GetOverlappedResult(hPipe, &ov, &written, 0);
        }
    }

    Sleep(50);
    CloseHandle(ov.hEvent);
    CloseHandle(hPipe);
    return 0;
}

static uint32_t __stdcall accept_one_client_thread(void* arg) {
    SOCKET listen_sock = (SOCKET)arg;
    SOCKET client_sock = accept(listen_sock, NULL, NULL);
    if (client_sock == INVALID_SOCKET) return 1;
    HANDLE hPipe = open_simconnect_pipe();
    if (hPipe == INVALID_HANDLE_VALUE) {
        closesocket(client_sock);
        return 1;
    }
    BridgeSession* s = &g_sessions[0];
    s->sock = client_sock;
    s->pipe = hPipe;
    s->active = 1;
    CreateThread(NULL, 0, pipe_to_tcp_thread, s, 0, NULL);
    CreateThread(NULL, 0, tcp_to_pipe_thread, s, 0, NULL);
    return 0;
}

void __stdcall mainCRTStartup(void) {
    const char* cmd = GetCommandLineA();

    if (str_contains(cmd, "--probe")) {
        for (int i = 0; DEFAULT_PIPES[i] != NULL; i++) {
            HANDLE h = CreateFileA(
                DEFAULT_PIPES[i],
                GENERIC_READ | GENERIC_WRITE,
                0,
                NULL,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                NULL
            );
            if (h != INVALID_HANDLE_VALUE) {
                CloseHandle(h);
                print_str("PIPE_FOUND:");
                print_str(DEFAULT_PIPES[i]);
                print_str("\n");
                ExitProcess(0);
            }
        }
        print_str("PIPE_NONE\n");
        ExitProcess(1);
    }

    WSADATA wsa;
    if (WSAStartup(0x0202, &wsa) != 0) {
        ExitProcess(2);
    }

    if (str_contains(cmd, "--self-test")) {
        g_custom_pipe = "\\\\.\\pipe\\AeroDeckSelfTestPipe";
        HANDLE hServerPipe = CreateNamedPipeA(
            g_custom_pipe,
            PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED,
            PIPE_TYPE_BYTE | PIPE_WAIT,
            4,
            16384,
            16384,
            1000,
            NULL
        );
        if (hServerPipe == INVALID_HANDLE_VALUE) {
            print_str("SELF_TEST_FAIL:CreateNamedPipeA\n");
            ExitProcess(3);
        }
        CreateThread(NULL, 0, mock_pipe_server_thread, (void*)hServerPipe, 0, NULL);

        SOCKET ls = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
        int opt = 1;
        setsockopt(ls, SOL_SOCKET, SO_REUSEADDR, (const char*)&opt, sizeof(opt));
        sockaddr_in addr;
        zero_mem(&addr, sizeof(addr));
        addr.sin_family = AF_INET;
        addr.sin_port = htons(45499);
        addr.sin_addr = inet_addr("127.0.0.1");
        if (bind(ls, &addr, sizeof(addr)) == SOCKET_ERROR || listen(ls, 4) == SOCKET_ERROR) {
            print_str("SELF_TEST_FAIL:bind\n");
            ExitProcess(4);
        }
        CreateThread(NULL, 0, accept_one_client_thread, (void*)ls, 0, NULL);

        SOCKET cs = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
        if (connect(cs, &addr, sizeof(addr)) == SOCKET_ERROR) {
            print_str("SELF_TEST_FAIL:connect\n");
            ExitProcess(5);
        }
        send(cs, "HELLO_PIPE", 10, 0);
        char rbuf[32];
        zero_mem(rbuf, sizeof(rbuf));
        int rn = recv(cs, rbuf, 16, 0);
        closesocket(cs);
        closesocket(ls);
        WSACleanup();

        if (rn == 16 && str_contains(rbuf, "SIMCONNECT_OK!!")) {
            print_str("SELF_TEST_OK\n");
            ExitProcess(0);
        } else {
            print_str("SELF_TEST_FAIL:recv\n");
            ExitProcess(6);
        }
    }

    uint16_t port = parse_u16(str_find_after(cmd, "--port"), 45454);

    SOCKET listen_sock = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (listen_sock == INVALID_SOCKET) {
        ExitProcess(3);
    }

    int opt = 1;
    setsockopt(listen_sock, SOL_SOCKET, SO_REUSEADDR, (const char*)&opt, sizeof(opt));

    sockaddr_in bind_addr;
    zero_mem(&bind_addr, sizeof(bind_addr));
    bind_addr.sin_family = AF_INET;
    bind_addr.sin_port = htons(port);
    bind_addr.sin_addr = inet_addr("127.0.0.1");

    if (bind(listen_sock, &bind_addr, sizeof(bind_addr)) == SOCKET_ERROR) {
        closesocket(listen_sock);
        ExitProcess(4);
    }

    if (listen(listen_sock, 8) == SOCKET_ERROR) {
        closesocket(listen_sock);
        ExitProcess(5);
    }

    print_str("BRIDGE_LISTENING\n");

    while (1) {
        SOCKET client_sock = accept(listen_sock, NULL, NULL);
        if (client_sock == INVALID_SOCKET) {
            Sleep(50);
            continue;
        }

        HANDLE hPipe = open_simconnect_pipe();
        if (hPipe == INVALID_HANDLE_VALUE) {
            closesocket(client_sock);
            continue;
        }

        BridgeSession* s = &g_sessions[g_session_idx % 16];
        g_session_idx++;
        s->sock = client_sock;
        s->pipe = hPipe;
        s->active = 1;

        HANDLE h1 = CreateThread(NULL, 0, pipe_to_tcp_thread, s, 0, NULL);
        HANDLE h2 = CreateThread(NULL, 0, tcp_to_pipe_thread, s, 0, NULL);
        if (h1) CloseHandle(h1);
        if (h2) CloseHandle(h2);
    }
}
