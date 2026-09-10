from __future__ import annotations

import os
import re
import tomllib
import unittest

ENV_KEY = re.compile(r"^[A-Z_][A-Z0-9_]{1,127}$")
MW_TOP = {"schema_version", "repository_mode", "allow_overlapping_roots", "default_target", "targets"}
MW_TARGET = {"name", "role", "roots", "enabled", "middleware", "stack_config", "propagate_headers"}


def parse(text: str) -> dict:
    value = tomllib.loads(text)
    if not isinstance(value, dict):
        raise ValueError("root-object-required")
    return value


def need(ok: bool, code: str) -> None:
    if not ok:
        raise ValueError(code)


def env_name(value: object, code: str) -> str:
    need(isinstance(value, str) and ENV_KEY.fullmatch(value) is not None, code)
    need("://" not in value and "=" not in value, code)
    return value


def validate_mw(v: dict) -> None:
    need(set(v) <= MW_TOP and v.get("schema_version") == 1, "mw:shape")
    mode = v.get("repository_mode")
    need(mode in {"client-only", "server-only", "hybrid"}, "mw:mode")
    targets = v.get("targets")
    need(isinstance(targets, list) and targets, "mw:targets")
    names: set[str] = set()
    roles: set[str] = set()
    roots = {"client": set(), "server": set()}
    for t in targets:
        need(isinstance(t, dict) and set(t) <= MW_TARGET, "mw:target-shape")
        name, role, middleware = t.get("name"), t.get("role"), t.get("middleware")
        need(isinstance(name, str) and name and name not in names, "mw:name")
        names.add(name)
        need(role in roots, "mw:role")
        roles.add(role)
        rs = t.get("roots")
        need(isinstance(rs, list) and rs and all(isinstance(x, str) and x for x in rs), "mw:roots")
        roots[role].update(rs)
        need(middleware in {"stack", "propagation-only", "disabled"}, "mw:middleware")
        if role == "client":
            need(middleware != "stack" and "stack_config" not in t, "mw:client-stack")
            if middleware == "propagation-only":
                hs = t.get("propagate_headers")
                need(isinstance(hs, list) and "traceparent" in hs, "mw:traceparent")
        if middleware == "stack":
            need(role == "server" and isinstance(t.get("stack_config"), str), "mw:stack-config")
    need(roles == ({"client"} if mode == "client-only" else {"server"} if mode == "server-only" else {"client", "server"}), "mw:role-leak")
    if mode == "hybrid" and roots["client"] & roots["server"]:
        need(v.get("allow_overlapping_roots") is True, "mw:overlap")
    if v.get("default_target") is not None:
        need(v["default_target"] in names, "mw:default-target")


def validate_rl(v: dict) -> None:
    need(v.get("schemaVersion") == "ores.rate-limit.config.v1", "rl:version")
    layout = v.get("layout")
    need(layout in {"client-only", "server-only", "combined"}, "rl:layout")
    client, server = v.get("client"), v.get("server")
    if layout in {"client-only", "combined"}:
        need(isinstance(client, dict), "rl:client")
        need(not ({"redisUrlEnv", "keyHmacEnv", "backend"} & set(client)), "rl:client-server-leak")
    if layout in {"server-only", "combined"}:
        need(isinstance(server, dict), "rl:server")
        for key in ("redisUrlEnv", "keyHmacEnv"):
            if key in server:
                env_name(server[key], f"rl:{key}")
    policies = v.get("policies")
    need(isinstance(policies, list) and policies, "rl:policies")
    for p in policies:
        need(isinstance(p, dict), "rl:policy")
        need(p.get("enforcementMode") in {"observe-only", "disabled"}, "rl:enforcement")
        need(p.get("consistencyMode") == "advisory", "rl:consistency")


def validate_lru(v: dict) -> None:
    need(v.get("protocol") == "ores.lru-config.v1", "lru:protocol")
    roles = v.get("roles")
    need(isinstance(roles, list) and roles and set(roles) <= {"client", "server"}, "lru:roles")
    if "redis" in v:
        need(isinstance(v["redis"], dict), "lru:redis")
        env_name(v["redis"].get("urlEnv"), "lru:url-env")
    caches = v.get("caches")
    need(isinstance(caches, list) and caches, "lru:caches")
    ids: set[tuple[str, str]] = set()
    for c in caches:
        need(isinstance(c, dict), "lru:cache")
        ident = (c.get("role"), c.get("name"))
        need(ident[0] in roles and isinstance(ident[1], str) and ident[1] and ident not in ids, "lru:identity")
        ids.add(ident)
        if ident[0] == "client":
            need(c.get("syncMode") == "local_only", "lru:client-local")


def validate_auth(v: dict, names: set[str]) -> None:
    need(not ({".shared-auth.toml", ".auth-shared.toml"} <= names), "auth:dual-alias")
    need(v.get("schema_version") == 1, "auth:version")
    c = v.get("compatibility")
    need(isinstance(c, dict) and c.get("repository") == "https://github.com/shared-auth/shared-auth-interfaces", "auth:provenance")
    need(isinstance(c.get("commit"), str) and re.fullmatch(r"[0-9a-f]{40}", c["commit"]) is not None, "auth:revision")
    forbidden = {"password", "private_key", "client_secret", "service_role_key", "database_url", "dsn"}
    stack = [v]
    while stack:
        x = stack.pop()
        if isinstance(x, dict):
            for k, child in x.items():
                need(str(k).lower() not in forbidden, "auth:secret-field")
                stack.append(child)
        elif isinstance(x, list):
            stack.extend(x)


def validate_domain(domain: dict, cli: dict) -> None:
    need(domain.get("version") == 1 and domain.get("mode") in {"client", "server", "hybrid"}, "domain:version-mode")
    need(domain.get("strict") is True, "domain:strict")
    f2e = domain.get("flags2env")
    need(isinstance(f2e, dict) and f2e.get("contract") == ".cli-flags.toml", "domain:flags-contract")
    need(f2e.get("require_audit") is True and f2e.get("precedence") == "argv-over-env", "domain:flags-policy")
    secret_envs: set[str] = set()
    for e in domain.get("env", []):
        need(isinstance(e, dict), "domain:env")
        key = env_name(e.get("key"), "domain:env-key")
        if e.get("secret") is True:
            need("default" not in e, "domain:secret-default")
            secret_envs.add(key)
    need(cli.get("env", {}).get("load") is False and cli.get("parse", {}).get("allow_unknown") is False, "flags2env:strict")
    flags = cli.get("flags", {})
    need(isinstance(flags, dict), "flags2env:flags")
    cli_envs = {x.get("env") for x in flags.values() if isinstance(x, dict)}
    need(not secret_envs & cli_envs, "flags2env:secret-argv")


MW_CLIENT = '''schema_version=1\nrepository_mode="client-only"\ndefault_target="client"\n[[targets]]\nname="client"\nrole="client"\nroots=["."]\nmiddleware="propagation-only"\npropagate_headers=["traceparent","baggage","x-request-id"]\n'''
MW_SERVER = '''schema_version=1\nrepository_mode="server-only"\n[[targets]]\nname="server"\nrole="server"\nroots=["."]\nmiddleware="disabled"\n'''
MW_HYBRID = '''schema_version=1\nrepository_mode="hybrid"\nallow_overlapping_roots=true\n[[targets]]\nname="server"\nrole="server"\nroots=["."]\nmiddleware="disabled"\n[[targets]]\nname="client"\nrole="client"\nroots=["."]\nmiddleware="propagation-only"\npropagate_headers=["traceparent","tracestate","baggage","x-request-id"]\n'''
MW_BAD_OVERLAP = MW_HYBRID.replace('allow_overlapping_roots=true\n', '')
MW_BAD_CLIENT_STACK = '''schema_version=1\nrepository_mode="client-only"\n[[targets]]\nname="client"\nrole="client"\nroots=["."]\nmiddleware="stack"\nstack_config="config/server.json"\n'''
RL_CLIENT = '''schemaVersion="ores.rate-limit.config.v1"\nlayout="client-only"\ndefaultPolicyId="test"\n[client]\nroot="."\nexposePolicyMetadata=true\n[[policies]]\npolicyId="test"\nenforcementMode="observe-only"\nconsistencyMode="advisory"\n'''
RL_COMBINED = '''schemaVersion="ores.rate-limit.config.v1"\nlayout="combined"\ndefaultPolicyId="test"\n[client]\nroot="."\nexposePolicyMetadata=true\n[server]\nroot="."\nbackend="local"\nkeyHmacEnv="ORES_RL_HMAC_KEY"\n[[policies]]\npolicyId="test"\nenforcementMode="observe-only"\nconsistencyMode="advisory"\n'''
RL_BAD_CLIENT = RL_CLIENT.replace('exposePolicyMetadata=true', 'exposePolicyMetadata=true\nredisUrlEnv="https://not-an-env-name.invalid"')
LRU_CLIENT = '''protocol="ores.lru-config.v1"\nroles=["client"]\n[[caches]]\nname="runtime-env"\nrole="client"\ncapacity=32\nsyncMode="local_only"\n'''
LRU_HYBRID = '''protocol="ores.lru-config.v1"\nroles=["client","server"]\n[redis]\nurlEnv="REDIS_URL"\nkeyPrefix="test:runtime"\npubsubChannel="test:runtime:events"\nreconcileIntervalMs=180000\nreconnectMinMs=1000\nreconnectMaxMs=30000\n[[caches]]\nname="runtime-env"\nrole="client"\ncapacity=32\nsyncMode="local_only"\n[[caches]]\nname="runtime-env"\nrole="server"\ncapacity=64\nsyncMode="read_only"\n'''
LRU_BAD = LRU_HYBRID.replace('urlEnv="REDIS_URL"', 'urlEnv="https://not-an-env-name.invalid"')
AUTH = '''schema_version=1\n[compatibility]\nrepository="https://github.com/shared-auth/shared-auth-interfaces"\ncommit="52b7ac7fbf0c7c169684f613eda923f3aa6c82e9"\n[factors.two_factor]\nrequired=false\nmethods=["totp","passkey"]\n[pages]\nshow=["sign-in","challenge","recovery","error"]\n[styling]\ntheme="system"\nbrand_name="Test Consumer"\naccent_color="#4F46E5"\n'''
DOMAIN = '''version=1\nmode="hybrid"\nstrict=true\n[flags2env]\ncontract=".cli-flags.toml"\nrequire_audit=true\nprecedence="argv-over-env"\n[[env]]\nname="api_base"\nkey="FANWAAVE_API_BASE"\nkind="url"\nrequired=false\nsecret=false\ndefault="http://127.0.0.1:8080"\n[[env]]\nname="auth_token"\nkey="FANWAAVE_AUTH_TOKEN"\nkind="string"\nrequired=true\nsecret=true\n'''
CLI = '''[env]\nload=false\n[parse]\nallow_unknown=false\n[flags.api-base]\nenv="FANWAAVE_API_BASE"\ntype="string"\n'''
CLI_BAD = CLI + '''[flags.auth-token]\nenv="FANWAAVE_AUTH_TOKEN"\ntype="string"\n'''


class RuntimeConfigSuiteTests(unittest.TestCase):
    def test_only_test_org(self):
        self.assertTrue(os.environ.get("TEST_GITHUB_ORG", "").endswith("-test"))

    def test_middleware_modes_and_negative_cases(self):
        for x in (MW_CLIENT, MW_SERVER, MW_HYBRID): validate_mw(parse(x))
        for x in (MW_BAD_OVERLAP, MW_BAD_CLIENT_STACK):
            with self.assertRaises(ValueError): validate_mw(parse(x))

    def test_rate_limit_projection(self):
        validate_rl(parse(RL_CLIENT)); validate_rl(parse(RL_COMBINED))
        with self.assertRaises(ValueError): validate_rl(parse(RL_BAD_CLIENT))

    def test_lru_projection(self):
        validate_lru(parse(LRU_CLIENT)); validate_lru(parse(LRU_HYBRID))
        with self.assertRaises(ValueError): validate_lru(parse(LRU_BAD))

    def test_shared_auth_alias(self):
        a = parse(AUTH); validate_auth(a, {".auth-shared.toml"})
        with self.assertRaises(ValueError): validate_auth(a, {".auth-shared.toml", ".shared-auth.toml"})

    def test_flags2env_secret_and_precedence(self):
        validate_domain(parse(DOMAIN), parse(CLI))
        with self.assertRaises(ValueError): validate_domain(parse(DOMAIN), parse(CLI_BAD))
        resolve = lambda default, env, argv: argv.get("K", env.get("K", default))
        self.assertEqual(resolve("default", {}, {}), "default")
        self.assertEqual(resolve("default", {"K": "env"}, {}), "env")
        self.assertEqual(resolve("default", {"K": "env"}, {"K": "argv"}), "argv")


if __name__ == "__main__": unittest.main()
