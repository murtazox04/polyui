#include <jni.h>
#include <limits.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "polyui.h"

#define POLYUI_JNI(name) Java_dev_polyui_PolyuiNative_##name

static void throw_named(JNIEnv *env, const char *class_name, const char *message) {
    if ((*env)->ExceptionCheck(env)) {
        return;
    }
    jclass type = (*env)->FindClass(env, class_name);
    if (type != NULL) {
        (*env)->ThrowNew(env, type, message);
        (*env)->DeleteLocalRef(env, type);
    }
}

static PolyuiClient *client_from(jlong handle) {
    return (PolyuiClient *)(intptr_t)handle;
}

static char *copy_in(JNIEnv *env, jbyteArray bytes) {
    if (bytes == NULL) {
        return NULL;
    }
    jsize length = (*env)->GetArrayLength(env, bytes);
    char *text = malloc((size_t)length + 1);
    if (text == NULL) {
        throw_named(env, "java/lang/OutOfMemoryError", "PolyUI could not copy an argument");
        return NULL;
    }
    (*env)->GetByteArrayRegion(env, bytes, 0, length, (jbyte *)text);
    text[length] = '\0';
    return text;
}

static jbyteArray take_out(JNIEnv *env, char *text) {
    if (text == NULL) {
        return NULL;
    }
    size_t length = strlen(text);
    if (length > INT_MAX) {
        polyui_string_free(text);
        throw_named(env, "java/lang/OutOfMemoryError", "PolyUI result is too large for a Java array");
        return NULL;
    }
    jbyteArray result = (*env)->NewByteArray(env, (jsize)length);
    if (result != NULL) {
        (*env)->SetByteArrayRegion(env, result, 0, (jsize)length, (const jbyte *)text);
    }
    polyui_string_free(text);
    return result;
}

static void store_error(JNIEnv *env, jobjectArray slot, char *error) {
    jbyteArray message = take_out(env, error);
    if (message == NULL || slot == NULL || (*env)->ExceptionCheck(env)) {
        return;
    }
    if ((*env)->GetArrayLength(env, slot) > 0) {
        (*env)->SetObjectArrayElement(env, slot, 0, message);
    }
    (*env)->DeleteLocalRef(env, message);
}

static jbyteArray finish(JNIEnv *env, char *result, char *error, jobjectArray slot) {
    if (result == NULL) {
        store_error(env, slot, error);
        return NULL;
    }
    polyui_string_free(error);
    return take_out(env, result);
}

static int copy_all(JNIEnv *env, jbyteArray *inputs, char **outputs, int count) {
    for (int index = 0; index < count; index++) {
        outputs[index] = NULL;
    }
    for (int index = 0; index < count; index++) {
        if (inputs[index] == NULL) {
            continue;
        }
        outputs[index] = copy_in(env, inputs[index]);
        if (outputs[index] == NULL) {
            for (int earlier = 0; earlier < index; earlier++) {
                free(outputs[earlier]);
            }
            return 0;
        }
    }
    return 1;
}

static void free_all(char **texts, int count) {
    for (int index = 0; index < count; index++) {
        free(texts[index]);
    }
}

JNIEXPORT jlong JNICALL POLYUI_JNI(clientNew)(JNIEnv *env, jclass type, jbyteArray hello,
                                               jboolean allow_http, jobjectArray slot) {
    (void)type;
    jbyteArray inputs[] = {hello};
    char *texts[1];
    if (!copy_all(env, inputs, texts, 1)) {
        return 0;
    }
    char *error = NULL;
    PolyuiClient *client = polyui_client_new(texts[0], allow_http == JNI_TRUE, &error);
    free_all(texts, 1);
    if (client == NULL) {
        store_error(env, slot, error);
        return 0;
    }
    polyui_string_free(error);
    return (jlong)(intptr_t)client;
}

JNIEXPORT jlong JNICALL POLYUI_JNI(clientNewWithCatalogs)(JNIEnv *env, jclass type, jbyteArray hello,
                                                           jbyteArray catalogs, jboolean allow_http,
                                                           jobjectArray slot) {
    (void)type;
    jbyteArray inputs[] = {hello, catalogs};
    char *texts[2];
    if (!copy_all(env, inputs, texts, 2)) {
        return 0;
    }
    char *error = NULL;
    PolyuiClient *client =
        polyui_client_new_with_catalogs(texts[0], texts[1], allow_http == JNI_TRUE, &error);
    free_all(texts, 2);
    if (client == NULL) {
        store_error(env, slot, error);
        return 0;
    }
    polyui_string_free(error);
    return (jlong)(intptr_t)client;
}

JNIEXPORT jbyteArray JNICALL POLYUI_JNI(receive)(JNIEnv *env, jclass type, jlong handle,
                                                  jbyteArray message, jobjectArray slot) {
    (void)type;
    jbyteArray inputs[] = {message};
    char *texts[1];
    if (!copy_all(env, inputs, texts, 1)) {
        return NULL;
    }
    char *error = NULL;
    char *result = polyui_client_receive(client_from(handle), texts[0], &error);
    free_all(texts, 1);
    return finish(env, result, error, slot);
}

JNIEXPORT jbyteArray JNICALL POLYUI_JNI(setValue)(JNIEnv *env, jclass type, jlong handle,
                                                   jbyteArray surface, jbyteArray path,
                                                   jbyteArray value, jobjectArray slot) {
    (void)type;
    jbyteArray inputs[] = {surface, path, value};
    char *texts[3];
    if (!copy_all(env, inputs, texts, 3)) {
        return NULL;
    }
    char *error = NULL;
    char *result = polyui_client_set_value(client_from(handle), texts[0], texts[1], texts[2], &error);
    free_all(texts, 3);
    return finish(env, result, error, slot);
}

JNIEXPORT jbyteArray JNICALL POLYUI_JNI(fire)(JNIEnv *env, jclass type, jlong handle,
                                               jbyteArray surface, jbyteArray component,
                                               jbyteArray prop, jbyteArray detail,
                                               jobjectArray slot) {
    (void)type;
    jbyteArray inputs[] = {surface, component, prop, detail};
    char *texts[4];
    if (!copy_all(env, inputs, texts, 4)) {
        return NULL;
    }
    char *error = NULL;
    char *result =
        polyui_client_fire(client_from(handle), texts[0], texts[1], texts[2], texts[3], &error);
    free_all(texts, 4);
    return finish(env, result, error, slot);
}

JNIEXPORT jbyteArray JNICALL POLYUI_JNI(confirm)(JNIEnv *env, jclass type, jlong handle,
                                                  jbyteArray surface, jbyteArray action_id,
                                                  jbyteArray nonce, jobjectArray slot) {
    (void)type;
    jbyteArray inputs[] = {surface, action_id, nonce};
    char *texts[3];
    if (!copy_all(env, inputs, texts, 3)) {
        return NULL;
    }
    char *error = NULL;
    char *result = polyui_client_confirm(client_from(handle), texts[0], texts[1], texts[2], &error);
    free_all(texts, 3);
    return finish(env, result, error, slot);
}

JNIEXPORT jbyteArray JNICALL POLYUI_JNI(say)(JNIEnv *env, jclass type, jlong handle,
                                              jbyteArray surface, jobjectArray slot) {
    (void)type;
    jbyteArray inputs[] = {surface};
    char *texts[1];
    if (!copy_all(env, inputs, texts, 1)) {
        return NULL;
    }
    char *error = NULL;
    char *result = polyui_client_say(client_from(handle), texts[0], &error);
    free_all(texts, 1);
    return finish(env, result, error, slot);
}

JNIEXPORT jbyteArray JNICALL POLYUI_JNI(matchVoice)(JNIEnv *env, jclass type, jlong handle,
                                                     jbyteArray surface, jbyteArray text,
                                                     jobjectArray slot) {
    (void)type;
    jbyteArray inputs[] = {surface, text};
    char *texts[2];
    if (!copy_all(env, inputs, texts, 2)) {
        return NULL;
    }
    char *error = NULL;
    char *result = polyui_client_match_voice(client_from(handle), texts[0], texts[1], &error);
    free_all(texts, 2);
    return finish(env, result, error, slot);
}

JNIEXPORT jbyteArray JNICALL POLYUI_JNI(embedPage)(JNIEnv *env, jclass type, jbyteArray props,
                                                    jbyteArray nonce, jobjectArray slot) {
    (void)type;
    jbyteArray inputs[] = {props, nonce};
    char *texts[2];
    if (!copy_all(env, inputs, texts, 2)) {
        return NULL;
    }
    char *error = NULL;
    char *result = polyui_embed_page(texts[0], texts[1], &error);
    free_all(texts, 2);
    return finish(env, result, error, slot);
}

JNIEXPORT jbyteArray JNICALL POLYUI_JNI(embedPageWith)(JNIEnv *env, jclass type, jbyteArray props,
                                                        jbyteArray nonce, jbyteArray host,
                                                        jobjectArray slot) {
    (void)type;
    jbyteArray inputs[] = {props, nonce, host};
    char *texts[3];
    if (!copy_all(env, inputs, texts, 3)) {
        return NULL;
    }
    char *error = NULL;
    char *result = polyui_embed_page_with(texts[0], texts[1], texts[2], &error);
    free_all(texts, 3);
    return finish(env, result, error, slot);
}

JNIEXPORT void JNICALL POLYUI_JNI(clientFree)(JNIEnv *env, jclass type, jlong handle) {
    (void)env;
    (void)type;
    polyui_client_free(client_from(handle));
}
