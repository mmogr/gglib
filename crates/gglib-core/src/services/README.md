# services

<!-- module-docs:start -->

Core services - the application's business logic layer.

This module contains high-level service abstractions that mediate
between ports (trait interfaces) and domain logic. Services reach
infrastructure through ports and don't know about concrete
implementations; model verification and the catalog's shard sizing read
model files on disk directly, and a repair deletes the files it fetches
again.

<!-- module-docs:end -->
