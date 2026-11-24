#ifndef POLYUI_H
#define POLYUI_H

#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct PolyuiClient PolyuiClient;

PolyuiClient *polyui_client_new(const char *hello_json, bool allow_http, char **error_out);

char *polyui_client_receive(PolyuiClient *client, const char *message_json, char **error_out);

char *polyui_client_set_value(PolyuiClient *client, const char *surface, const char *path,
                              const char *value_json, char **error_out);

char *polyui_client_fire(PolyuiClient *client, const char *surface, const char *component,
                         const char *prop, const char *detail_json, char **error_out);

char *polyui_client_confirm(PolyuiClient *client, const char *surface, const char *action_id,
                            const char *nonce, char **error_out);

char *polyui_client_say(PolyuiClient *client, const char *surface, char **error_out);

char *polyui_client_match_voice(PolyuiClient *client, const char *surface, const char *text,
                                char **error_out);

void polyui_string_free(char *text);

void polyui_client_free(PolyuiClient *client);

#ifdef __cplusplus
}
#endif

#endif
