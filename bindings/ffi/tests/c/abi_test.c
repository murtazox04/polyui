#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "polyui.h"

static int failures = 0;

static void check(int condition, const char *what) {
    if (!condition) {
        fprintf(stderr, "FAIL: %s\n", what);
        failures += 1;
    }
}

static int contains(const char *text, const char *part) {
    return text != NULL && strstr(text, part) != NULL;
}

static const char *hello =
    "{\"v\":1,\"t\":\"hello\",\"client\":{\"name\":\"c-test\",\"version\":\"1\"},"
    "\"catalogs\":[\"core@1\",\"chart@1\"],\"modalities\":[\"screen\",\"voice\"],\"locale\":\"en\"}";

static const char *open_message =
    "{\"v\":1,\"t\":\"ui.open\",\"surface\":\"s1\",\"seq\":1,\"root\":\"card\",\"components\":["
    "{\"id\":\"card\",\"type\":\"Card\",\"props\":{\"title\":\"Bills\"},\"children\":[\"pick\",\"phone\",\"pay\"]},"
    "{\"id\":\"pick\",\"type\":\"ChoiceGroup\",\"props\":{\"label\":\"Pay from\",\"value\":\"/card\","
    "\"options\":[{\"id\":\"humo\",\"label\":\"Humo\"},{\"id\":\"visa\",\"label\":\"Visa\"}]}},"
    "{\"id\":\"phone\",\"type\":\"Input\",\"props\":{\"label\":\"Phone\",\"value\":\"/phone\",\"input\":\"phone\"}},"
    "{\"id\":\"pay\",\"type\":\"Button\",\"props\":{\"label\":\"Pay\",\"tone\":\"primary\","
    "\"on_press\":{\"action_id\":\"act_pay\",\"confirm\":true,\"with\":[\"/phone\"]}}}],\"data\":{}}";

static const char *acme_hello =
    "{\"v\":1,\"t\":\"hello\",\"client\":{\"name\":\"c-test\",\"version\":\"1\"},"
    "\"catalogs\":[\"core@1\",\"acme@1\"],\"modalities\":[\"screen\"],\"locale\":\"en\"}";

static const char *acme_catalogs =
    "[{\"id\":\"acme\",\"version\":1,\"types\":{\"Badge2\":{\"description\":\"A status badge.\","
    "\"role\":\"status\",\"props\":{\"text\":{\"kind\":\"string\",\"required\":true}},"
    "\"alt\":\"{text}\"}}}]";

static const char *acme_open =
    "{\"v\":1,\"t\":\"ui.open\",\"surface\":\"s2\",\"seq\":1,\"root\":\"b\",\"components\":["
    "{\"id\":\"b\",\"type\":\"acme:Badge2\",\"props\":{\"text\":\"Paid\"}}],\"data\":{}}";

static const char *confirm_message =
    "{\"v\":1,\"t\":\"ui.confirm\",\"surface\":\"s1\",\"action_id\":\"act_pay\",\"nonce\":\"n_1\","
    "\"title\":\"Pay\",\"expires_ms\":60000}";

int main(void) {
    char *error = NULL;

    PolyuiClient *broken = polyui_client_new("{\"v\":1}", false, &error);
    check(broken == NULL, "a bad hello gives no client");
    check(contains(error, "could not start a client"), "a bad hello gives an error");
    polyui_string_free(error);
    error = NULL;

    PolyuiClient *client = polyui_client_new(hello, false, &error);
    check(client != NULL && error == NULL, "client_new succeeds");
    if (client == NULL) {
        return 1;
    }

    char *batch = polyui_client_receive(client, open_message, &error);
    check(contains(batch, "\"op\":\"create\""), "receive gives create ops");
    check(contains(batch, "\"op\":\"root\""), "receive gives a root op");
    check(error == NULL, "receive has no error");
    polyui_string_free(batch);

    char *update = polyui_client_set_value(client, "s1", "/phone", "\"+998901234567\"", &error);
    check(contains(update, "+998901234567"), "set_value returns the bound update");
    polyui_string_free(update);

    char *bad_value = polyui_client_set_value(client, "s1", "/phone", "{nope", &error);
    check(bad_value == NULL, "a bad value gives no result");
    check(contains(error, "not valid JSON"), "a bad value gives an error");
    polyui_string_free(error);
    error = NULL;

    char *action = polyui_client_fire(client, "s1", "pay", "on_press", NULL, &error);
    check(contains(action, "\"t\":\"ui.action\""), "fire returns a ui.action");
    check(contains(action, "+998901234567"), "fire copies bound values");
    polyui_string_free(action);

    char *with_detail = polyui_client_fire(client, "s1", "pay", "on_press", "{\"row\":2}", &error);
    check(contains(with_detail, "\"detail\":{\"row\":2}"), "fire copies the detail");
    polyui_string_free(with_detail);

    char *missing = polyui_client_fire(client, "s1", "nobody", "on_press", NULL, &error);
    check(missing != NULL && strcmp(missing, "null") == 0, "fire on an unknown component is null");
    polyui_string_free(missing);

    char *challenge = polyui_client_receive(client, confirm_message, &error);
    check(contains(challenge, "\"op\":\"confirm\""), "ui.confirm becomes a confirm op");
    polyui_string_free(challenge);

    char *confirmed = polyui_client_confirm(client, "s1", "act_pay", "n_1", &error);
    check(contains(confirmed, "\"confirm\":\"n_1\""), "confirm returns the action with the nonce");
    polyui_string_free(confirmed);

    char *said = polyui_client_say(client, "s1", &error);
    check(contains(said, "\"id\":\"card\""), "say returns utterances");
    polyui_string_free(said);

    char *matched = polyui_client_match_voice(client, "s1", "the second one", &error);
    check(contains(matched, "\"component\":\"pick\""), "match_voice finds the choice");
    check(contains(matched, "\"option\":\"visa\""), "match_voice picks the second option");
    polyui_string_free(matched);

    char *page = polyui_embed_page("{\"resource\":\"ui://a/b\",\"title\":\"</script>\"}",
                                   "abcdefghijklmnop0123", &error);
    check(contains(page, "const nonce = \"abcdefghijklmnop0123\""), "embed_page puts the nonce in");
    check(contains(page, "\\u003c/script\\u003e"), "embed_page escapes the props for a script");
    polyui_string_free(page);

    char *bad_page = polyui_embed_page("{}", "short", &error);
    check(bad_page == NULL, "a short nonce gives no page");
    check(contains(error, "nonce"), "a short nonce is named");
    polyui_string_free(error);
    error = NULL;

    char *null_client = polyui_client_say(NULL, "s1", &error);
    check(null_client == NULL, "a null client gives no result");
    check(contains(error, "the client is null"), "a null client is named");
    polyui_string_free(error);
    error = NULL;

    char *null_arg = polyui_client_receive(client, NULL, NULL);
    check(null_arg == NULL, "a null message with no error slot is safe");

    const char invalid_utf8[] = {'{', (char)0xff, '}', 0};
    char *bad_text = polyui_client_receive(client, invalid_utf8, &error);
    check(bad_text == NULL, "invalid UTF-8 gives no result");
    check(contains(error, "not valid UTF-8"), "invalid UTF-8 is named");
    polyui_string_free(error);

    PolyuiClient *plain = polyui_client_new(acme_hello, false, &error);
    char *as_text = polyui_client_receive(plain, acme_open, &error);
    check(contains(as_text, "\"type\":\"Text\""), "without its catalog an extension type falls back to Text");
    polyui_string_free(as_text);
    polyui_client_free(plain);

    PolyuiClient *extended = polyui_client_new_with_catalogs(acme_hello, acme_catalogs, false, &error);
    check(extended != NULL && error == NULL, "client_new_with_catalogs succeeds");
    char *as_badge = polyui_client_receive(extended, acme_open, &error);
    check(contains(as_badge, "\"type\":\"acme:Badge2\""), "with its catalog an extension type keeps its type");
    check(contains(as_badge, "\"text\":\"Paid\""), "with its catalog an extension type keeps its props");
    polyui_string_free(as_badge);
    polyui_client_free(extended);

    PolyuiClient *rejected = polyui_client_new_with_catalogs(acme_hello, "[{\"id\":\"core\",\"version\":1,\"types\":{}}]", false, &error);
    check(rejected == NULL, "a reserved catalog id gives no client");
    check(contains(error, "an extension catalog was rejected: [0].id"), "a reserved catalog id is named");
    polyui_string_free(error);
    error = NULL;

    polyui_string_free(NULL);
    polyui_client_free(client);
    polyui_client_free(NULL);

    if (failures == 0) {
        printf("c abi test: all checks passed\n");
        return 0;
    }
    fprintf(stderr, "c abi test: %d checks failed\n", failures);
    return 1;
}
