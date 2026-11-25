from collections.abc import Callable, Mapping, Sequence
from typing import Any, Literal, TypeAlias, TypedDict, final

from typing_extensions import NotRequired

__version__: str

Json: TypeAlias = dict[str, Any]
Payload: TypeAlias = Mapping[str, Any] | str
Operation: TypeAlias = Literal["show", "update", "close"]
Resolver: TypeAlias = Callable[[Any, str, str], Any]
ToolProfile: TypeAlias = Literal["compact", "full"]
UiResourceReader: TypeAlias = Callable[[Any, str], Mapping[str, Any] | None]
IdFactory: TypeAlias = Callable[[], str]

class Problem(TypedDict):
    at: str
    message: str
    field: NotRequired[str]
    list: NotRequired[str]
    index: NotRequired[int]
    id: NotRequired[str]
    prop: NotRequired[str]
    path: NotRequired[str]

SealedAction = TypedDict(
    "SealedAction",
    {
        "action_id": str,
        "component": str,
        "prop": str,
        "action": str,
        "args": dict[str, Any],
        "with": list[str],
        "requires_confirm": bool,
    },
)

class Component(TypedDict):
    id: str
    type: str
    props: NotRequired[dict[str, Any]]
    children: NotRequired[list[str]]
    alt: NotRequired[str]
    say: NotRequired[str]
    voice: NotRequired[dict[str, Any]]

class PropSpec(TypedDict, total=False):
    kind: str
    required: bool
    default: Any
    bindable: bool
    description: str
    min: float
    max: float
    values: list[str]
    schemes: list[str]
    of: PropSpec
    fields: dict[str, PropSpec]

class ActionSpec(TypedDict, total=False):
    args: dict[str, PropSpec]
    requires_confirm: bool
    description: str

class PolyUIError(Exception): ...

class ValidationError(PolyUIError):
    problems: list[Problem]
    operation: Operation

class CatalogError(PolyUIError):
    path: str

class ChartError(PolyUIError): ...
class ProtocolError(PolyUIError): ...
class RefRefusedError(PolyUIError): ...

@final
class Catalogs:
    @staticmethod
    def builtin() -> Catalogs: ...
    @staticmethod
    def empty() -> Catalogs: ...
    def with_embed(self) -> Catalogs: ...
    def with_catalog(self, catalog: Mapping[str, Any] | str) -> Catalogs: ...
    def restrict(self, declared: Sequence[str]) -> Catalogs: ...
    def negotiate(self, declared: Sequence[str]) -> list[str]: ...
    @property
    def tags(self) -> list[str]: ...
    def type_names(self) -> list[str]: ...
    def __contains__(self, type_name: str) -> bool: ...
    def __len__(self) -> int: ...

@final
class Surface:
    @staticmethod
    def from_dict(value: Mapping[str, Any]) -> Surface: ...
    def to_dict(self) -> Json: ...
    @property
    def id(self) -> str | None: ...
    @property
    def root(self) -> str: ...
    @property
    def components(self) -> list[Component]: ...
    @property
    def data(self) -> Json: ...
    def get(self, id: str) -> Component | None: ...
    def apply(self, update: Update) -> Surface: ...
    def __len__(self) -> int: ...

@final
class Update:
    def to_dict(self) -> Json: ...
    @property
    def surface(self) -> str: ...
    @property
    def upsert(self) -> list[Component]: ...
    @property
    def remove(self) -> list[str]: ...
    @property
    def root(self) -> str | None: ...
    @property
    def data(self) -> list[Json]: ...

@final
class Prepared:
    @property
    def surface_id(self) -> str: ...
    @property
    def model(self) -> Surface: ...
    @property
    def client(self) -> Surface: ...
    @property
    def messages(self) -> list[Json]: ...
    @property
    def summary(self) -> str: ...
    @property
    def actions(self) -> list[SealedAction]: ...
    @property
    def seq(self) -> int: ...
    @property
    def touched(self) -> list[str]: ...

@final
class Validator:
    def __new__(
        cls,
        catalogs: Catalogs,
        actions: Mapping[str, ActionSpec] | None = None,
        ref_sources: Mapping[str, str | PropSpec] | None = None,
        resolver: Resolver | None = None,
        allow_http: bool = False,
        ui_resource: UiResourceReader | None = None,
    ) -> Validator: ...
    @property
    def catalogs(self) -> Catalogs: ...
    @property
    def ref_sources(self) -> list[str]: ...
    def show_ui(self, payload: Payload) -> Surface: ...
    def update_ui(self, payload: Payload, current: Surface) -> Update: ...
    def close_ui(self, payload: Payload) -> str: ...
    def resolve_refs(
        self, surface: Surface, user: Any = None, refs: Mapping[str, Any] | None = None
    ) -> Surface: ...
    def seal_actions(
        self, surface: Surface, next_id: IdFactory | None = None
    ) -> tuple[Surface, list[SealedAction]]: ...
    def compile_charts(
        self,
        surface: Surface,
        locale: str = "en",
        width: int | None = None,
        height: int | None = None,
    ) -> Surface: ...
    def attach_apps(self, surface: Surface, user: Any = None) -> Surface: ...
    def downgrade(
        self, surface: Surface, declared: Sequence[str], locale: str = "en"
    ) -> Surface: ...
    def summary(
        self, surface: Surface, surface_id: str | None = None, locale: str = "en"
    ) -> str: ...
    def fallback(self, surface: Surface, locale: str = "en") -> str: ...
    def prepare_show(
        self,
        payload: Payload,
        user: Any = None,
        surface_id: str | None = None,
        declared: Sequence[str] | None = None,
        locale: str = "en",
        next_id: IdFactory | None = None,
        refs: Mapping[str, Any] | None = None,
    ) -> Prepared: ...
    def prepare_update(
        self,
        payload: Payload,
        model: Surface,
        client: Surface,
        seq: int,
        user: Any = None,
        declared: Sequence[str] | None = None,
        locale: str = "en",
        next_id: IdFactory | None = None,
        refs: Mapping[str, Any] | None = None,
    ) -> Prepared: ...
    def ref_keys(self, payload: Payload) -> list[str]: ...
    def tool_definitions(
        self,
        declared: Sequence[str] | None = None,
        strict: bool = False,
        profile: ToolProfile | None = None,
    ) -> list[Json]: ...

@final
class Client:
    def __new__(
        cls,
        hello: Mapping[str, Any] | str,
        catalogs: Catalogs | None = None,
        allow_http: bool = False,
    ) -> Client: ...
    @property
    def locale(self) -> str: ...
    @property
    def session(self) -> str | None: ...
    @property
    def surfaces(self) -> list[str]: ...
    def data(self, surface: str) -> Any | None: ...
    def fallback(self, surface: str) -> str | None: ...
    def receive(self, message: Mapping[str, Any] | str) -> Json: ...
    def set_value(self, surface: str, path: str, value: Any) -> Json: ...
    def fire(
        self,
        surface: str,
        component: str,
        prop: str,
        detail: Mapping[str, Any] | None = None,
    ) -> Json | None: ...
    def confirm(self, surface: str, action_id: str, nonce: str) -> Json | None: ...
    def say(self, surface: str) -> list[Json]: ...
    def match_voice(self, surface: str, text: str) -> Json | None: ...

def tool_definitions(
    catalogs: Catalogs,
    actions: Mapping[str, ActionSpec] | None = None,
    strict: bool = False,
    profile: ToolProfile | None = None,
) -> list[Json]: ...
def open_message(
    surface_id: str, surface: Surface, seq: int = 1, fallback: str | None = None
) -> Json: ...
def patch_message(
    surface_id: str,
    seq: int,
    upsert: Sequence[Component] | None = None,
    remove: Sequence[str] = (),
    root: str | None = None,
) -> Json: ...
def close_message(surface_id: str, seq: int) -> Json: ...
def closed_summary(surface_id: str) -> str: ...
def compile_chart(
    props: Mapping[str, Any],
    locale: str = "en",
    width: int | None = None,
    height: int | None = None,
) -> Json: ...
def chart_svg(
    props: Mapping[str, Any],
    locale: str = "en",
    width: int | None = None,
    height: int | None = None,
    dark: bool = False,
) -> str: ...
def describe_chart(props: Mapping[str, Any], locale: str = "en") -> str: ...
