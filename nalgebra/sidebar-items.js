initSidebarItems({"enum":[["POrdering","Result of a partial ordering."]],"trait":[["Absolute","Trait of objects having an absolute value. This is useful if the object does not have the same type as its absolute value."],["AbsoluteRotate","Composition of a rotation and an absolute value."],["ApproxEq","Trait for testing approximate equality"],["Axpy","Trait of objects implementing the `y = ax + y` operation."],["BaseFloat","Basic floating-point number numeric trait."],["BaseNum","Basic integral numeric trait."],["Basis","Traits of objects which can form a basis (typically vectors)."],["Bounded","Types that have maximum and minimum value."],["Cast","Traits of objects which can be created from an object of type `T`."],["Col","Trait to access columns of a matrix or vector."],["ColSlice","Trait to access part of a column of a matrix"],["Cov","Trait for computing the covariance of a set of data."],["Cross","Trait of elements having a cross product."],["CrossMatrix","Trait of elements having a cross product operation which can be expressed as a matrix."],["Det","Trait of objects having a determinant. Typically used by square matrices."],["Diag","Trait to get the diagonal of square matrices."],["Dim","Trait of objects having a spacial dimension known at compile time."],["Dot","Traits of objects having a dot product."],["EigenQR","Trait for computing the eigenvector and eigenvalues of a square matrix usin the QR algorithm."],["Eye","Trait for constructing the identity matrix"],["FloatPnt","Trait of points with components implementing the `BaseFloat` trait."],["FloatVec","Trait of vector with components implementing the `BaseFloat` trait."],["FromHomogeneous","Traits of objects which can be build from an homogeneous coordinate form."],["Indexable","This is a workaround of current Rust limitations."],["Inv","Trait of objects having an inverse. Typically used to implement matrix inverse."],["Iterable","This is a workaround of current Rust limitations."],["IterableMut","This is a workaround of current Rust limitations."],["LMul","Trait of objects having a left multiplication with another element."],["Mat","Trait of matrices."],["Mean","Trait for computing the covariance of a set of data."],["Norm","Traits of objects having an euclidian norm."],["NumPnt","Trait grouping most common operations on points."],["NumVec","Trait grouping most common operations on vectors."],["Orig","The zero element of a vector space, seen as an element of its embeding affine space."],["Outer","Traits of objects having an outer product."],["POrd","Pointwise ordering operations."],["PntAsVec","Trait that relates a point of an affine space to a vector of the associated vector space."],["RMul","Trait of objects having a right multiplication with another element."],["Rotate","Trait of objects able to rotate other objects."],["Rotation","Trait of object which can represent a rotation, and to which new rotations can be appended. A rotation is assumed to be an isometry without translation and without reflexion."],["RotationMatrix","Trait of transformation having a rotation extractable as a rotation matrix. This can typically be implemented by quaternions to convert them to a rotation matrix."],["RotationWithTranslation","Various composition of rotation and translation."],["Row","Trait to access rows of a matrix or a vector."],["RowSlice","Trait to access part of a row of a matrix"],["ScalarAdd","Trait of objects having an addition with a scalar."],["ScalarDiv","Trait of objects having a division by a scalar."],["ScalarMul","Trait of objects having a multiplication with a scalar."],["ScalarSub","Trait of objects having a subtraction with a scalar."],["Shape","The shape of an indexable object."],["SquareMat","Trait implemented by square matrices."],["ToHomogeneous","Traits of objects which can be put in homogeneous coordinates form."],["Transform","Trait of objects able to transform other objects."],["Transformation","Trait of object which represent a transformation, and to which new transformations can be appended."],["Translate","Trait of objects able to translate other objects. This is typically implemented by vectors to translate points."],["Translation","Trait of object which represent a translation, and to wich new translation can be appended."],["Transpose","Trait of objects which can be transposed."],["UniformSphereSample","Trait of vectors able to sample a unit sphere."]],"macro":[["assert_approx_eq!","Asserts approximate equality of two values with the `ApproxEq` trait."],["assert_approx_eq_eps!","Asserts approximate equality within a given tolerance of two values with the `ApproxEq` trait."],["assert_approx_eq_ulps!","Asserts approximate equality within a given tolerance of two values with the `ApproxEq` trait, with tolerance specified in ULPs."]],"struct":[["DMat","Matrix with dimensions unknown at compile-time."],["DVec","Heap allocated, dynamically sized vector."],["DVec1","Stack-allocated, dynamically sized vector with a maximum size of 1."],["DVec2","Stack-allocated, dynamically sized vector with a maximum size of 2."],["DVec3","Stack-allocated, dynamically sized vector with a maximum size of 3."],["DVec4","Stack-allocated, dynamically sized vector with a maximum size of 4."],["DVec5","Stack-allocated, dynamically sized vector with a maximum size of 5."],["DVec6","Stack-allocated, dynamically sized vector with a maximum size of 6."],["Identity","Special identity matrix. All its operation are no-ops."],["Iso2","Two dimensional isometry."],["Iso3","Three dimensional isometry."],["Iso4","Four dimensional isometry."],["Mat1","Square matrix of dimension 1."],["Mat2","Square matrix of dimension 2."],["Mat3","Square matrix of dimension 3."],["Mat4","Square matrix of dimension 4."],["Mat5","Square matrix of dimension 5."],["Mat6","Square matrix of dimension 6."],["Ortho3","A 3D orthographic projection stored without any matrix."],["OrthoMat3","A 3D orthographic projection stored as a 4D matrix."],["Persp3","A 3D perspective projection stored without any matrix."],["PerspMat3","A 3D perspective projection stored as a 4D matrix."],["Pnt0","Point of dimension 0."],["Pnt1","Point of dimension 1."],["Pnt2","Point of dimension 2."],["Pnt3","Point of dimension 3."],["Pnt4","Point of dimension 4."],["Pnt5","Point of dimension 5."],["Pnt6","Point of dimension 6."],["Quat","A quaternion."],["Rot2","Two dimensional rotation matrix."],["Rot3","Three dimensional rotation matrix."],["Rot4","Four dimensional rotation matrix."],["UnitQuat","A unit quaternion that can represent a 3D rotation."],["Vec0","Vector of dimension 0."],["Vec1","Vector of dimension 1."],["Vec2","Vector of dimension 2."],["Vec3","Vector of dimension 3."],["Vec4","Vector of dimension 4."],["Vec5","Vector of dimension 5."],["Vec6","Vector of dimension 6."]],"fn":[["abs","Computes a component-wise absolute value."],["absolute_rotate","Applies a rotation using the absolute values of its components."],["append_rotation","Applies the rotation `v` to a copy of `m`."],["append_rotation_wrt_center","Rotates a copy of `m` by `amount` using `m.translation()` as the pivot point."],["append_rotation_wrt_point","Rotates a copy of `m` by `amount` using `center` as the pivot point."],["append_transformation","Gets a transformed copy of `m`."],["append_translation","Applies the translation `v` to a copy of `m`."],["approx_eq","Tests approximate equality."],["approx_eq_eps","Tests approximate equality using a custom epsilon."],["canonical_basis","Computes the canonical basis for a given dimension."],["canonical_basis_element","Gets the (0-based) i-th element of the canonical basis of V."],["cast","Converts an object from one type to another."],["center","Returns the center of two points."],["clamp","Change the input value to ensure it is on the range `[min, max]`."],["cov","Computes the covariance of a set of observations."],["cross","Computes the cross product of two vectors."],["cross_matrix","Given a vector, computes the matrix which, when multiplied by another vector, computes a cross product."],["det","Computes the determinant of a square matrix."],["diag","Gets the diagonal of a square matrix."],["dim","Gets the dimension an object lives in."],["dist","Returns the distance between two points."],["dot","Computes the dot product of two vectors."],["eigen_qr","Computes the eigenvalues and eigenvectors of a square matrix usin the QR algorithm."],["from_homogeneous","Converts a matrix or vector from homogeneous coordinates."],["householder_matrix","Get the householder matrix corresponding to a reflexion to the hyperplane defined by `vec`. It can be a reflexion contained in a subspace."],["identity","Create a special identity object."],["inf","Returns the infimum of `a` and `b`."],["inv","Gets an inverted copy of a matrix."],["inv_rotate","Applies an inverse rotation to a vector."],["inv_rotation","Gets the inverse rotation applicable by `m`."],["inv_transform","Applies an inverse transformation to a vector."],["inv_transformation","Gets the inverse transformation applicable by `m`."],["inv_translate","Applies an inverse translation to a point."],["inv_translation","Gets the inverse translation applicable by `m`."],["is_zero","Tests is a value is iqual to zero."],["max","Same as `cmp::max`."],["mean","Computes the mean of a set of observations."],["min","Same as `cmp::min`."],["new_identity","Construct the identity matrix for a given dimension"],["norm","Computes the L2 norm of a vector."],["normalize","Gets the normalized version of a vector."],["one","Create a one-valued value."],["orig","Returns the trivial origin of an affine space."],["orthonormal_subspace_basis","Computes the basis of the orthonormal subspace of a given vector."],["outer","Computes the outer product of two vectors."],["partial_clamp","Clamp `value` between `min` and `max`. Returns `None` if `value` is not comparable to `min` or `max`."],["partial_cmp","Compare `a` and `b` using a partial ordering relation."],["partial_ge","Returns `true` iff `a` and `b` are comparable and `a >= b`."],["partial_gt","Returns `true` iff `a` and `b` are comparable and `a > b`."],["partial_le","Returns `true` iff `a` and `b` are comparable and `a <= b`."],["partial_lt","Returns `true` iff `a` and `b` are comparable and `a < b`."],["partial_max","Return the maximum of `a` and `b` if they are comparable."],["partial_min","Return the minimum of `a` and `b` if they are comparable."],["prepend_rotation","Pre-applies the rotation `v` to a copy of `m`."],["qr","QR decomposition using Householder reflections."],["rotate","Applies a rotation to a vector."],["rotation","Gets the rotation applicable by `m`."],["sample_sphere","Samples the unit sphere living on the dimension as the samples types."],["shape","Gets the indexable range of an object."],["sqdist","Returns the squared distance between two points."],["sqnorm","Computes the squared L2 norm of a vector."],["sup","Returns the supremum of `a` and `b`."],["to_homogeneous","Converts a matrix or vector to homogeneous coordinates."],["to_rot_mat","Builds a rotation matrix from `r`."],["transform","Applies a transformation to a vector."],["transformation","Gets the transformation applicable by `m`."],["translate","Applies a translation to a point."],["translation","Gets the translation applicable by `m`."],["transpose","Gets a transposed copy of a matrix."],["zero","Create a zero-valued value."]]});