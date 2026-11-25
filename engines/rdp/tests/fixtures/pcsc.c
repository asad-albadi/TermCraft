/* Private Smart Card API fixture. The system PC/SC library and bus are unused. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#ifdef VERIFY_PCSC_HEADERS
#include <winscard.h>
#else
typedef unsigned long DWORD;
typedef long LONG;
typedef LONG SCARDCONTEXT;
typedef LONG SCARDHANDLE;
typedef struct {
    const char *szReader;
    void *pvUserData;
    DWORD dwCurrentState;
    DWORD dwEventState;
    DWORD cbAtr;
    unsigned char rgbAtr[33];
} SCARD_READERSTATE;
typedef struct {
    DWORD dwProtocol;
    DWORD cbPciLength;
} SCARD_IO_REQUEST;
#endif

#define INVALID_HANDLE ((LONG)0x80100003)
#define INVALID_VALUE ((LONG)0x80100011)
#define INSUFFICIENT ((LONG)0x80100008)
#define TRANSIENT ((LONG)0x80100016)
#define TIMEOUT ((LONG)0x8010000a)
#define READER "Fixture Reader 00 00"
#if __SIZEOF_LONG__ > 4
#define HANDLE_BASE 0x123450000L
#else
#define HANDLE_BASE 0x12345000L
#endif

static LONG context_serial = HANDLE_BASE;
static LONG card_serial = HANDLE_BASE + 1000;
static SCARDCONTEXT active_contexts[32];
static SCARDHANDLE active_cards[32];
static SCARDCONTEXT card_owners[32];

static int index_of(const LONG *handles, LONG target) {
    for (int i = 0; i < 32; ++i) {
        if (handles[i] == target) return i;
    }
    return -1;
}

LONG SCardEstablishContext(DWORD scope, const void *a, const void *b, SCARDCONTEXT *context) {
    if (scope != 0 || a || b) return INVALID_VALUE;
    int slot = index_of(active_contexts, 0);
    if (slot < 0) return INVALID_VALUE;
    *context = active_contexts[slot] = ++context_serial;
    return 0;
}

LONG SCardIsValidContext(SCARDCONTEXT context) {
    return context && index_of(active_contexts, context) >= 0 ? 0 : INVALID_HANDLE;
}

LONG SCardReleaseContext(SCARDCONTEXT context) {
    int slot = index_of(active_contexts, context);
    if (!context || slot < 0) return INVALID_HANDLE;
    if (context > HANDLE_BASE + 1 && getenv("TERMCRAFT_PCSC_FIXTURE_RELEASE_FAIL")) return TRANSIENT;
    active_contexts[slot] = 0;
    for (int i = 0; i < 32; ++i) {
        if (card_owners[i] == context) active_cards[i] = card_owners[i] = 0;
    }
    return 0;
}

LONG SCardCancel(SCARDCONTEXT context) {
    if (getenv("TERMCRAFT_PCSC_FIXTURE_CANCEL_FAIL")) return TRANSIENT;
    return SCardIsValidContext(context);
}

LONG SCardListReaders(SCARDCONTEXT context, const char *groups, char *readers, DWORD *length) {
    static const char names[] = READER "\0Unselected Reader 01 00\0";
    if (SCardIsValidContext(context) || groups) return INVALID_HANDLE;
    if (*length < sizeof(names)) return INSUFFICIENT;
    *length = sizeof(names);
    memcpy(readers, names, sizeof(names));
    return 0;
}

LONG SCardConnect(SCARDCONTEXT context, const char *reader, DWORD share, DWORD protocols,
                  SCARDHANDLE *card, DWORD *protocol) {
    if (SCardIsValidContext(context)) return INVALID_HANDLE;
    if (strcmp(reader, READER) != 0 || share != 2 || !(protocols & 2)) return INVALID_VALUE;
    int slot = index_of(active_cards, 0);
    if (slot < 0) return INVALID_VALUE;
    *card = active_cards[slot] = ++card_serial;
    card_owners[slot] = context;
    *protocol = 2;
    return 0;
}

LONG SCardDisconnect(SCARDHANDLE card, DWORD disposition) {
    int slot = index_of(active_cards, card);
    if (!card || slot < 0 || disposition > 3) return INVALID_HANDLE;
    active_cards[slot] = card_owners[slot] = 0;
    return 0;
}

LONG SCardBeginTransaction(SCARDHANDLE card) {
    return card && index_of(active_cards, card) >= 0 ? 0 : INVALID_HANDLE;
}

LONG SCardEndTransaction(SCARDHANDLE card, DWORD disposition) {
    return disposition <= 3 ? SCardBeginTransaction(card) : INVALID_VALUE;
}

LONG SCardStatus(SCARDHANDLE card, char *reader, DWORD *reader_len, DWORD *state, DWORD *protocol,
                 unsigned char *atr, DWORD *atr_len) {
    if (SCardBeginTransaction(card)) return INVALID_HANDLE;
    if (*reader_len < sizeof(READER) || *atr_len < 2) return INSUFFICIENT;
    memcpy(reader, READER, sizeof(READER));
    *reader_len = sizeof(READER);
    *state = 64;
    *protocol = 2;
    atr[0] = 0x3b;
    atr[1] = 0x01;
    *atr_len = 2;
    return 0;
}

LONG SCardGetStatusChange(SCARDCONTEXT context, DWORD timeout, SCARD_READERSTATE *state, DWORD count) {
    if (SCardIsValidContext(context)) return INVALID_HANDLE;
    if (timeout != 0 || count != 1 || strcmp(state->szReader, READER) != 0) return INVALID_VALUE;
    const char *changed = getenv("TERMCRAFT_PCSC_FIXTURE_CHANGED");
    if (state->dwCurrentState == 0x20 && (!changed || access(changed, F_OK) != 0)) return TIMEOUT;
    state->dwEventState = 0x22;
    state->cbAtr = 2;
    state->rgbAtr[0] = 0x3b;
    state->rgbAtr[1] = 0x01;
    return 0;
}

LONG SCardTransmit(SCARDHANDLE card, const SCARD_IO_REQUEST *send, const unsigned char *request,
                   DWORD request_len, SCARD_IO_REQUEST *receive, unsigned char *response,
                   DWORD *response_len) {
    const char *calls = getenv("TERMCRAFT_PCSC_FIXTURE_CALLS");
    if (calls) {
        FILE *log = fopen(calls, "a");
        if (log) {
            fputc('x', log);
            fclose(log);
        }
    }
    const unsigned char fixture_request[] = {0xfa, 0xce, 0x21};
    if (SCardBeginTransaction(card)) return INVALID_HANDLE;
    if (send->dwProtocol != 2 || send->cbPciLength != sizeof(SCARD_IO_REQUEST)) return INVALID_VALUE;
    if (request_len != sizeof(fixture_request) || memcmp(request, fixture_request, request_len))
        return INVALID_VALUE;
    const char *stall = getenv("TERMCRAFT_PCSC_FIXTURE_STALL");
    if (stall) {
        FILE *entered = fopen(stall, "w");
        if (entered) fclose(entered);
        for (;;) pause();
    }
    if (*response_len < 3) return INSUFFICIENT;
    if (receive) {
        if (receive->cbPciLength != sizeof(SCARD_IO_REQUEST)) return INVALID_VALUE;
        receive->dwProtocol = 2;
    }
    response[0] = 0x2a;
    response[1] = 0x90;
    response[2] = 0x00;
    *response_len = 3;
    return 0;
}
